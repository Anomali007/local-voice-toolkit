use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex as TokioMutex;

use crate::audio::playback::{PlaybackProgress, StreamingPlayer};
use crate::audio::stretch::{time_stretch, MAX_SPEED, MIN_SPEED};
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

/// Stop the reading in progress, if any (sync; used by the Escape handler).
pub fn stop_reading() {
    stop_current();
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

/// Speed used for chunks synthesized from now on (f32 bits). Seeded from the
/// requested speed when a reading starts; `set_tts_speed` changes it live.
static LIVE_SPEED: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

fn clamp_speed(speed: f32) -> f32 {
    if speed.is_finite() {
        speed.clamp(MIN_SPEED, MAX_SPEED)
    } else {
        1.0
    }
}

fn live_speed() -> f32 {
    f32::from_bits(LIVE_SPEED.load(Ordering::SeqCst))
}

fn set_live_speed(speed: f32) {
    LIVE_SPEED.store(clamp_speed(speed).to_bits(), Ordering::SeqCst);
}

/// How much of the reading is synthesized, for the elapsed/remaining display.
#[derive(Debug, Default, Clone)]
pub struct ReadProgress {
    chunk_count: usize,
    chars_total: usize,
    chunks_done: usize,
    chars_done: usize,
    /// Audio length of the finished chunks before time-stretch.
    native_secs: f64,
    /// Audio length of the finished chunks as queued (stretched).
    queued_secs: f64,
}

impl ReadProgress {
    pub fn new(chunks: &[String]) -> Self {
        Self {
            chunk_count: chunks.len(),
            chars_total: chunks.iter().map(|c| c.chars().count()).sum(),
            ..Self::default()
        }
    }

    pub fn record(&mut self, chars: usize, native_secs: f64, queued_secs: f64) {
        self.chunks_done += 1;
        self.chars_done += chars;
        self.native_secs += native_secs;
        self.queued_secs += queued_secs;
    }

    /// Total reading time in seconds and whether it is still an estimate.
    /// Unsynthesized text is priced at the speaking rate measured so far
    /// (characters per unstretched second), then divided by the speed the
    /// next chunks will use. `0.0` means nothing is known yet.
    pub fn total_secs(&self, speed: f32) -> (f64, bool) {
        if self.chunk_count > 0 && self.chunks_done >= self.chunk_count {
            return (self.queued_secs, false);
        }
        if self.chars_done == 0 || self.native_secs <= 0.0 {
            return (0.0, true);
        }
        let secs_per_char = self.native_secs / self.chars_done as f64;
        let remaining_chars = self.chars_total.saturating_sub(self.chars_done) as f64;
        let speed = clamp_speed(speed) as f64;
        (self.queued_secs + remaining_chars * secs_per_char / speed, true)
    }
}

#[derive(Serialize, Clone, Debug)]
struct TtsProgress {
    elapsed_s: f64,
    total_s: f64,
    total_estimated: bool,
    speed: f32,
    chunk_index: usize,
    chunk_count: usize,
}

fn progress_payload(read: &ReadProgress, playback: &PlaybackProgress, speed: f32) -> TtsProgress {
    let elapsed = playback.position_secs();
    let (total, estimated) = read.total_secs(speed);
    TtsProgress {
        elapsed_s: elapsed,
        // The estimate can lag behind real playback; never show negative time left.
        total_s: if total > 0.0 { total.max(elapsed) } else { 0.0 },
        total_estimated: estimated,
        speed,
        chunk_index: (playback.chunks_done() + 1).min(read.chunk_count.max(1)),
        chunk_count: read.chunk_count,
    }
}

/// Shows the reading overlay (and arms Escape) for the life of a reading;
/// dropping it on any exit path hides the overlay again.
struct ReadingSession {
    app: AppHandle,
    overlay_generation: Option<u64>,
    done: Arc<AtomicBool>,
}

impl ReadingSession {
    fn begin(app: &AppHandle) -> Self {
        let overlay_generation = match crate::overlay::show_reading_overlay(app) {
            Ok(generation) => generation,
            Err(e) => {
                tracing::warn!("Could not show reading overlay: {}", e);
                None
            }
        };
        crate::hotkeys::arm_read_escape(app);
        // Tells the overlay UI to switch to its reading face.
        let _ = app.emit("tts-reading", overlay_generation.is_some());
        Self { app: app.clone(), overlay_generation, done: Arc::new(AtomicBool::new(false)) }
    }

    /// Emit `tts-progress` about 4 times a second until the reading ends.
    fn spawn_ticker(&self, read: Arc<Mutex<ReadProgress>>, playback: Arc<PlaybackProgress>) {
        let app = self.app.clone();
        let done = Arc::clone(&self.done);
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
            while !done.load(Ordering::SeqCst) {
                tick.tick().await;
                let payload = {
                    let read = read.lock().unwrap_or_else(|p| p.into_inner());
                    progress_payload(&read, &playback, live_speed())
                };
                let _ = app.emit("tts-progress", payload);
            }
        });
    }
}

impl Drop for ReadingSession {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        if let Some(generation) = self.overlay_generation {
            // A dictation that started meanwhile owns the overlay now.
            if crate::overlay::overlay_generation() == generation {
                let _ = crate::overlay::hide_overlay(&self.app);
            }
        }
        crate::hotkeys::disarm_read_escape(&self.app);
        let _ = self.app.emit("tts-reading", false);
    }
}

/// Read `text` aloud. Resolves when playback has finished or was stopped.
/// Starting a new reading stops the previous one.
#[tauri::command]
pub async fn speak_text(
    app: AppHandle,
    text: String,
    voice_id: String,
    speed: f32,
    engine: Option<String>,
) -> Result<(), String> {
    speak_text_impl(Some(app), text, voice_id, speed, engine).await
}

/// `app` is `None` only in tests (no window to show).
pub(crate) async fn speak_text_impl(
    app: Option<AppHandle>,
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
    set_live_speed(speed);
    // Shown before the (possibly slow) engine load so the user sees it working.
    let session = app.as_ref().map(ReadingSession::begin);
    ensure_engine(kind).await?;

    let stop = Arc::new(AtomicBool::new(false));
    *current() = Some(Arc::clone(&stop));
    let player = StreamingPlayer::start(Arc::clone(&stop));
    let read = Arc::new(Mutex::new(ReadProgress::new(&chunks)));
    if let Some(session) = &session {
        session.spawn_ticker(Arc::clone(&read), player.progress());
    }

    let result = synthesize_into(&player, chunks, voice_id, &read, &stop).await;
    if result.is_err() {
        stop.store(true, Ordering::SeqCst);
    }
    let played = tokio::task::spawn_blocking(move || player.finish_and_wait())
        .await
        .map_err(|e| format!("Playback crashed: {}", e))
        .and_then(|r| r.map_err(|e| format!("Failed to play audio: {}", e)));
    clear_if_current(&stop);
    drop(session);

    result?;
    played
}

/// Synthesize chunk by chunk, queueing each for playback as soon as it is
/// ready, so the first sentence plays while the rest are generated. Each
/// chunk is stretched at the speed in force when it is synthesized, so a
/// speed change applies to everything not yet synthesized.
async fn synthesize_into(
    player: &StreamingPlayer,
    chunks: Vec<String>,
    voice_id: String,
    read: &Arc<Mutex<ReadProgress>>,
    stop: &Arc<AtomicBool>,
) -> Result<(), String> {
    let state = Arc::clone(engine_state());
    for chunk in chunks {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let state = Arc::clone(&state);
        let voice = voice_id.clone();
        let chars = chunk.chars().count();
        let speed = live_speed();
        let (samples, rate, native_secs) = tokio::task::spawn_blocking(move || {
            let mut guard = state.blocking_lock();
            let engine = guard.as_mut().ok_or_else(|| "TTS engine not initialized".to_string())?;
            let (samples, rate) = engine
                .synthesize(&chunk, &voice)
                .map_err(|e| format!("Speech synthesis failed: {}", e))?;
            let native_secs = samples.len() as f64 / rate.max(1) as f64;
            Ok::<_, String>((time_stretch(&samples, rate, speed), rate, native_secs))
        })
        .await
        .map_err(|e| format!("Synthesis crashed: {}", e))??;

        let queued_secs = samples.len() as f64 / rate.max(1) as f64;
        read.lock().unwrap_or_else(|p| p.into_inner()).record(chars, native_secs, queued_secs);
        if !player.push(samples, rate) {
            break;
        }
    }
    Ok(())
}

/// Change the reading speed: persisted, and applied to every chunk
/// synthesized from now on (audio already queued finishes at the old speed).
pub fn apply_speed(app: &AppHandle, speed: f32) -> Result<f32, String> {
    let speed = (clamp_speed(speed) * 20.0).round() / 20.0; // 0.05 steps
    set_live_speed(speed);
    let mut settings = crate::commands::settings::get_settings().unwrap_or_default();
    settings.tts_speed = speed;
    crate::commands::settings::update_settings(app.clone(), settings)?;
    let _ = app.emit("settings-changed", ());
    Ok(speed)
}

#[tauri::command]
pub fn set_tts_speed(app: AppHandle, speed: f32) -> Result<f32, String> {
    apply_speed(&app, speed)
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
        let err = speak_text_impl(None, "  **  ".into(), "af_heart".into(), 1.0, Some("kokoro".into()))
            .await
            .unwrap_err();
        assert!(err.contains("Nothing to read"));
    }

    #[test]
    fn total_is_unknown_until_a_chunk_is_done() {
        let p = ReadProgress::new(&["aaaa".to_string(), "bbbb".to_string()]);
        assert_eq!(p.total_secs(1.0), (0.0, true));
    }

    #[test]
    fn remaining_text_is_priced_at_the_measured_rate_and_speed() {
        // 4 chunks of 100 chars; first synthesized: 5 s native, played at 1x.
        let chunks: Vec<String> = (0..4).map(|_| "x".repeat(100)).collect();
        let mut p = ReadProgress::new(&chunks);
        p.record(100, 5.0, 5.0);
        let (total, estimated) = p.total_secs(1.0);
        assert!(estimated);
        assert!((total - 20.0).abs() < 1e-9, "{total}");
        // Speeding up to 2x shortens only the unsynthesized part: 5 + 15/2.
        let (total, _) = p.total_secs(2.0);
        assert!((total - 12.5).abs() < 1e-9, "{total}");
        // A chunk already queued at 1x keeps its length after a speed change.
        p.record(100, 5.0, 2.5); // second chunk synthesized at 2x
        let (total, _) = p.total_secs(2.0);
        assert!((total - (7.5 + 200.0 * 0.05 / 2.0)).abs() < 1e-9, "{total}");
    }

    #[test]
    fn total_is_exact_once_every_chunk_is_known() {
        let chunks = vec!["abc".to_string(), "defg".to_string()];
        let mut p = ReadProgress::new(&chunks);
        p.record(3, 1.0, 0.5);
        p.record(4, 2.0, 1.0);
        assert_eq!(p.total_secs(1.0), (1.5, false));
    }

    #[test]
    fn payload_carries_chunk_counts_and_flags_the_estimate() {
        let mut p = ReadProgress::new(&["x".repeat(10), "x".repeat(10)]);
        p.record(10, 1.0, 1.0);
        let playback = PlaybackProgress::default();
        let payload = progress_payload(&p, &playback, 1.0);
        assert_eq!((payload.chunk_index, payload.chunk_count), (1, 2));
        assert!(payload.total_estimated && payload.total_s > 0.0);
    }

    #[test]
    fn speed_is_clamped_to_the_supported_range() {
        assert_eq!(clamp_speed(0.1), 0.5);
        assert_eq!(clamp_speed(9.0), 3.0);
        assert_eq!(clamp_speed(f32::NAN), 1.0);
        set_live_speed(1.7);
        assert_eq!(live_speed(), 1.7);
        set_live_speed(1.0);
    }
}
