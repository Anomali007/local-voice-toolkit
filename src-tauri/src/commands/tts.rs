use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::audio::playback::AudioPlayer;

#[derive(Default)]
pub struct TtsState {
    pub is_speaking: AtomicBool,
    pub player: Mutex<Option<AudioPlayer>>,
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
    state: State<'_, Arc<TtsState>>,
    text: String,
    voice_id: String,
    speed: f32,
    _model_path: String,
) -> Result<(), String> {
    if state.is_speaking.load(Ordering::SeqCst) {
        return Err("Already speaking".to_string());
    }

    tracing::info!("Speaking text with voice {}: {}", voice_id, text);
    state.is_speaking.store(true, Ordering::SeqCst);

    // TODO: Integrate kokoroxide TTS engine
    // For now, this is a placeholder that will be implemented
    // when the TTS engine is integrated

    // Placeholder: Generate silence for testing
    let sample_rate = 24000;
    let duration_secs = 1.0;
    let samples: Vec<f32> = vec![0.0; (sample_rate as f32 * duration_secs * speed) as usize];

    let mut player_guard = state.player.lock().await;
    let player = AudioPlayer::new().map_err(|e| e.to_string())?;
    player.play(&samples, sample_rate).map_err(|e| e.to_string())?;
    *player_guard = Some(player);

    state.is_speaking.store(false, Ordering::SeqCst);
    tracing::info!("Finished speaking");

    Ok(())
}

#[tauri::command]
pub async fn stop_speaking(state: State<'_, Arc<TtsState>>) -> Result<(), String> {
    if !state.is_speaking.load(Ordering::SeqCst) {
        return Ok(());
    }

    tracing::info!("Stopping speech...");

    let mut player_guard = state.player.lock().await;
    if let Some(player) = player_guard.take() {
        player.stop();
    }

    state.is_speaking.store(false, Ordering::SeqCst);
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
