use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Mutex as TokioMutex;

use crate::audio::playback::AudioPlayer;
use crate::engines::kokoro::KokoroEngine;

// Global player instance for stop functionality
static CURRENT_PLAYER: OnceLock<Arc<Mutex<Option<AudioPlayer>>>> = OnceLock::new();

// Global TTS engine cache - lazy initialized on first use
// Using tokio Mutex for async initialization
static TTS_ENGINE: OnceLock<Arc<TokioMutex<Option<KokoroEngine>>>> = OnceLock::new();

fn get_player_state() -> &'static Arc<Mutex<Option<AudioPlayer>>> {
    CURRENT_PLAYER.get_or_init(|| Arc::new(Mutex::new(None)))
}

fn get_tts_engine_state() -> &'static Arc<TokioMutex<Option<KokoroEngine>>> {
    TTS_ENGINE.get_or_init(|| Arc::new(TokioMutex::new(None)))
}

fn get_models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("models")
        .join("tts")
}

async fn get_or_init_tts_engine() -> Result<(), String> {
    let state = get_tts_engine_state();
    let mut guard = state.lock().await;

    if guard.is_none() {
        let model_dir = get_models_dir();
        tracing::info!("Initializing TTS engine from: {:?}", model_dir);

        let engine = KokoroEngine::new(model_dir.clone())
            .await
            .map_err(|e| format!("Failed to initialize TTS engine from {:?}: {}", model_dir, e))?;
        *guard = Some(engine);
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
}

#[tauri::command]
pub async fn speak_text(
    text: String,
    voice_id: String,
    speed: f32,
    _model_path: String,
) -> Result<(), String> {
    tracing::info!("Speaking text with voice {}: {}", voice_id, text);

    // Initialize TTS engine if not already done
    get_or_init_tts_engine().await?;

    // Synthesize speech
    let audio_buffer = {
        let state = get_tts_engine_state();
        let mut guard = state.lock().await;
        let engine = guard
            .as_mut()
            .ok_or_else(|| "TTS engine not initialized".to_string())?;

        engine
            .synthesize(&text, &voice_id, speed)
            .map_err(|e| format!("Speech synthesis failed for voice '{}': {}", voice_id, e))?
    };

    let player = AudioPlayer::new()
        .map_err(|e| format!("Failed to initialize audio player: {}", e))?;

    // Store player for potential stop
    {
        let mut guard = get_player_state().lock()
            .map_err(|e| format!("Internal error: audio player state lock poisoned: {}", e))?;
        *guard = Some(AudioPlayer::new()
            .map_err(|e| format!("Failed to create backup audio player: {}", e))?);
    }

    player
        .play(audio_buffer.samples(), audio_buffer.sample_rate)
        .map_err(|e| format!("Failed to play audio: {}", e))?;

    tracing::info!(
        "Started speaking ({:.2}s of audio)",
        audio_buffer.duration_secs()
    );
    Ok(())
}

#[tauri::command]
pub async fn stop_speaking() -> Result<(), String> {
    tracing::info!("Stopping speech...");

    let mut guard = get_player_state().lock()
        .map_err(|e| format!("Internal error: audio player state lock poisoned: {}", e))?;
    if let Some(player) = guard.take() {
        player.stop();
    }

    Ok(())
}

#[tauri::command]
pub fn get_voices() -> Vec<VoiceInfo> {
    // Kokoro-82M voices - subset of the 54 available
    vec![
        VoiceInfo {
            id: "af_heart".to_string(),
            name: "Heart".to_string(),
            language: "en-US".to_string(),
            gender: "Female".to_string(),
        },
        VoiceInfo {
            id: "af_bella".to_string(),
            name: "Bella".to_string(),
            language: "en-US".to_string(),
            gender: "Female".to_string(),
        },
        VoiceInfo {
            id: "af_nicole".to_string(),
            name: "Nicole".to_string(),
            language: "en-US".to_string(),
            gender: "Female".to_string(),
        },
        VoiceInfo {
            id: "af_sky".to_string(),
            name: "Sky".to_string(),
            language: "en-US".to_string(),
            gender: "Female".to_string(),
        },
        VoiceInfo {
            id: "am_adam".to_string(),
            name: "Adam".to_string(),
            language: "en-US".to_string(),
            gender: "Male".to_string(),
        },
        VoiceInfo {
            id: "am_michael".to_string(),
            name: "Michael".to_string(),
            language: "en-US".to_string(),
            gender: "Male".to_string(),
        },
        VoiceInfo {
            id: "bf_emma".to_string(),
            name: "Emma".to_string(),
            language: "en-GB".to_string(),
            gender: "Female".to_string(),
        },
        VoiceInfo {
            id: "bm_george".to_string(),
            name: "George".to_string(),
            language: "en-GB".to_string(),
            gender: "Male".to_string(),
        },
    ]
}
