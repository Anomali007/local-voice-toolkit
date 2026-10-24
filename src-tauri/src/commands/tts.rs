use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Mutex as TokioMutex;

use crate::audio::playback::StreamingPlayer;
use crate::audio::stretch::time_stretch;
use crate::engines::kokoro::KokoroEngine;
use crate::engines::phonon::{self, PhononEngine};
use crate::engines::text::{clean_for_speech, split_for_speech, MAX_CHUNK_CHARS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Kokoro,
    Phonon,
}

impl EngineKind {
    pub fn parse(name: &str) -> Self {
        if name.eq_ignore_ascii_case("phonon") {
            Self::Phonon
        } else {
            Self::Kokoro
        }
    }
}

enum LoadedEngine {
    Kokoro(KokoroEngine),
    Phonon(PhononEngine),
}

impl LoadedEngine {
    fn kind(&self) -> EngineKind {
        match self {
            Self::Kokoro(_) => EngineKind::Kokoro,
            Self::Phonon(_) => EngineKind::Phonon,
        }
    }

    /// Synthesize one chunk at the engine's native rate.
    fn synthesize(&mut self, text: &str, voice_id: &str) -> anyhow::Result<(Vec<f32>, u32)> {
        match self {
            Self::Kokoro(engine) => {
                let voice = if is_kokoro_voice(voice_id) { voice_id } else { DEFAULT_KOKORO_VOICE };
                let buffer = engine.synthesize(text, voice)?;
                Ok((buffer.samples, buffer.sample_rate))
            }
            Self::Phonon(engine) => Ok((engine.synthesize(text)?, engine.sample_rate())),
        }
    }
}

const DEFAULT_KOKORO_VOICE: &str = "af_heart";

// The engine is loaded on first use and kept until a different one is picked.
static TTS_ENGINE: OnceLock<Arc<TokioMutex<Option<LoadedEngine>>>> = OnceLock::new();

// Stop flag of the reading in progress, if any.
static CURRENT: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn engine_state() -> &'static Arc<TokioMutex<Option<LoadedEngine>>> {
    TTS_ENGINE.get_or_init(|| Arc::new(TokioMutex::new(None)))
}

fn current() -> std::sync::MutexGuard<'static, Option<Arc<AtomicBool>>> {
    CURRENT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn get_models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("models")
        .join("tts")
}

async fn ensure_engine(kind: EngineKind) -> Result<(), String> {
    let mut guard = engine_state().lock().await;
    if guard.as_ref().map(LoadedEngine::kind) == Some(kind) {
        return Ok(());
    }
    // Free the old engine before loading the next one.
    *guard = None;

    let models_dir = get_models_dir();
    tracing::info!("Initializing {:?} TTS engine from: {:?}", kind, models_dir);
    let engine = match kind {
        EngineKind::Kokoro => LoadedEngine::Kokoro(
            KokoroEngine::new(models_dir.clone()).await.map_err(|e| e.to_string())?,
        ),
        EngineKind::Phonon => {
            let dir = models_dir.join(phonon::MODEL_ID);
            let engine = tokio::task::spawn_blocking(move || PhononEngine::new(dir))
                .await
                .map_err(|e| format!("Phonon loader crashed: {}", e))?
                .map_err(|e| e.to_string())?;
            LoadedEngine::Phonon(engine)
        }
    };
    *guard = Some(engine);
    Ok(())
}

/// True while a reading is in progress (synthesizing or playing).
pub fn is_speaking() -> bool {
    current().as_ref().is_some_and(|flag| !flag.load(Ordering::SeqCst))
}

fn stop_current() {
    if let Some(flag) = current().take() {
        flag.store(true, Ordering::SeqCst);
    }
}

fn clear_if_current(flag: &Arc<AtomicBool>) {
    let mut guard = current();
    if guard.as_ref().is_some_and(|f| Arc::ptr_eq(f, flag)) {
        *guard = None;
    }
}

/// Read `text` aloud. Resolves when playback has finished or was stopped.
/// Starting a new reading stops the previous one.
#[tauri::command]
pub async fn speak_text(
    text: String,
    voice_id: String,
    speed: f32,
    engine: Option<String>,
) -> Result<(), String> {
    let kind = match engine {
        Some(name) => EngineKind::parse(&name),
        None => EngineKind::parse(
            &crate::commands::settings::get_settings().map(|s| s.tts_engine).unwrap_or_default(),
        ),
    };
    tracing::info!("Speaking {} chars with {:?} voice {} at {}x", text.len(), kind, voice_id, speed);

    let chunks = split_for_speech(&clean_for_speech(&text), MAX_CHUNK_CHARS);
    if chunks.is_empty() {
        return Err("Nothing to read.".into());
    }

    stop_current();
    ensure_engine(kind).await?;

    let stop = Arc::new(AtomicBool::new(false));
    *current() = Some(Arc::clone(&stop));
    let player = StreamingPlayer::start(Arc::clone(&stop));

    let result = synthesize_into(&player, chunks, voice_id, speed, &stop).await;
    if result.is_err() {
        stop.store(true, Ordering::SeqCst);
    }
    let played = tokio::task::spawn_blocking(move || player.finish_and_wait())
        .await
        .map_err(|e| format!("Playback crashed: {}", e))
        .and_then(|r| r.map_err(|e| format!("Failed to play audio: {}", e)));
    clear_if_current(&stop);

    result?;
    played
}

/// Synthesize chunk by chunk, queueing each for playback as soon as it is
/// ready, so the first sentence plays while the rest are generated.
async fn synthesize_into(
    player: &StreamingPlayer,
    chunks: Vec<String>,
    voice_id: String,
    speed: f32,
    stop: &Arc<AtomicBool>,
) -> Result<(), String> {
    let state = Arc::clone(engine_state());
    for chunk in chunks {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let state = Arc::clone(&state);
        let voice = voice_id.clone();
        let (samples, rate) = tokio::task::spawn_blocking(move || {
            let mut guard = state.blocking_lock();
            let engine = guard.as_mut().ok_or_else(|| "TTS engine not initialized".to_string())?;
            let (samples, rate) = engine
                .synthesize(&chunk, &voice)
                .map_err(|e| format!("Speech synthesis failed: {}", e))?;
            Ok::<_, String>((time_stretch(&samples, rate, speed), rate))
        })
        .await
        .map_err(|e| format!("Synthesis crashed: {}", e))??;

        if !player.push(samples, rate) {
            break;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn stop_speaking() -> Result<(), String> {
    tracing::info!("Stopping speech...");
    stop_current();
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
}

fn is_kokoro_voice(id: &str) -> bool {
    kokoro_voices().iter().any(|v| v.id == id)
}

/// Kokoro voices. kokoro-tiny phonemizes English only, so other languages'
/// voices would read English phonemes; they are left out until that changes.
fn kokoro_voices() -> Vec<VoiceInfo> {
    let v = |id: &str, name: &str, language: &str, gender: &str| VoiceInfo {
        id: id.into(),
        name: name.into(),
        language: language.into(),
        gender: gender.into(),
    };
    vec![
        // American English - Female
        v("af_heart", "Heart", "en-US", "Female"),
        v("af_alloy", "Alloy", "en-US", "Female"),
        v("af_aoede", "Aoede", "en-US", "Female"),
        v("af_bella", "Bella", "en-US", "Female"),
        v("af_jessica", "Jessica", "en-US", "Female"),
        v("af_kore", "Kore", "en-US", "Female"),
        v("af_nicole", "Nicole", "en-US", "Female"),
        v("af_nova", "Nova", "en-US", "Female"),
        v("af_river", "River", "en-US", "Female"),
        v("af_sarah", "Sarah", "en-US", "Female"),
        v("af_sky", "Sky", "en-US", "Female"),
        // American English - Male
        v("am_adam", "Adam", "en-US", "Male"),
        v("am_echo", "Echo", "en-US", "Male"),
        v("am_eric", "Eric", "en-US", "Male"),
        v("am_fable", "Fable", "en-US", "Male"),
        v("am_liam", "Liam", "en-US", "Male"),
        v("am_michael", "Michael", "en-US", "Male"),
        v("am_onyx", "Onyx", "en-US", "Male"),
        v("am_puck", "Puck", "en-US", "Male"),
        // British English - Female
        v("bf_alice", "Alice", "en-GB", "Female"),
        v("bf_emma", "Emma", "en-GB", "Female"),
        v("bf_isabella", "Isabella", "en-GB", "Female"),
        v("bf_lily", "Lily", "en-GB", "Female"),
        // British English - Male
        v("bm_daniel", "Daniel", "en-GB", "Male"),
        v("bm_fable", "Fable (UK)", "en-GB", "Male"),
        v("bm_george", "George", "en-GB", "Male"),
        v("bm_lewis", "Lewis", "en-GB", "Male"),
    ]
}

#[tauri::command]
pub fn get_voices(engine: Option<String>) -> Vec<VoiceInfo> {
    match EngineKind::parse(engine.as_deref().unwrap_or("kokoro")) {
        EngineKind::Kokoro => kokoro_voices(),
        EngineKind::Phonon => phonon::voices()
            .into_iter()
            .map(|v| VoiceInfo { id: v.id, name: v.name, language: v.language, gender: "Neutral".into() })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_kind_parses_and_defaults_to_kokoro() {
        assert_eq!(EngineKind::parse("phonon"), EngineKind::Phonon);
        assert_eq!(EngineKind::parse("Phonon"), EngineKind::Phonon);
        assert_eq!(EngineKind::parse("kokoro"), EngineKind::Kokoro);
        assert_eq!(EngineKind::parse(""), EngineKind::Kokoro);
        assert_eq!(EngineKind::parse("nonsense"), EngineKind::Kokoro);
    }

    #[test]
    fn voices_per_engine() {
        let kokoro = get_voices(None);
        assert_eq!(kokoro.len(), 27);
        assert!(kokoro.iter().all(|v| v.language.starts_with("en-")));
        assert!(is_kokoro_voice("af_heart") && !is_kokoro_voice("default"));
        let phonon = get_voices(Some("phonon".into()));
        assert_eq!(phonon.len(), 1);
        assert_eq!(phonon[0].id, "default");
    }

    #[test]
    fn stop_flags_the_reading_in_progress() {
        // This is the bug the old code had: stop flagged a different player.
        let flag = Arc::new(AtomicBool::new(false));
        *current() = Some(Arc::clone(&flag));
        assert!(is_speaking());
        stop_current();
        assert!(flag.load(Ordering::SeqCst), "stop must reach the active reading");
        assert!(!is_speaking());

        // A finished reading only clears itself, never a newer one.
        let older = Arc::new(AtomicBool::new(false));
        let newer = Arc::new(AtomicBool::new(false));
        *current() = Some(Arc::clone(&newer));
        clear_if_current(&older);
        assert!(is_speaking());
        clear_if_current(&newer);
        assert!(!is_speaking());
    }

    #[tokio::test]
    async fn empty_text_is_rejected_before_loading_anything() {
        let err = speak_text("  **  ".into(), "af_heart".into(), 1.0, Some("kokoro".into()))
            .await
            .unwrap_err();
        assert!(err.contains("Nothing to read"));
    }
}
