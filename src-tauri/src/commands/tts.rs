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
    vec![
        // American English - Female
        VoiceInfo { id: "af_heart".into(), name: "Heart".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_alloy".into(), name: "Alloy".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_aoede".into(), name: "Aoede".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_bella".into(), name: "Bella".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_jessica".into(), name: "Jessica".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_kore".into(), name: "Kore".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_nicole".into(), name: "Nicole".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_nova".into(), name: "Nova".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_river".into(), name: "River".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_sarah".into(), name: "Sarah".into(), language: "en-US".into(), gender: "Female".into() },
        VoiceInfo { id: "af_sky".into(), name: "Sky".into(), language: "en-US".into(), gender: "Female".into() },
        // American English - Male
        VoiceInfo { id: "am_adam".into(), name: "Adam".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_echo".into(), name: "Echo".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_eric".into(), name: "Eric".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_fable".into(), name: "Fable".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_liam".into(), name: "Liam".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_michael".into(), name: "Michael".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_onyx".into(), name: "Onyx".into(), language: "en-US".into(), gender: "Male".into() },
        VoiceInfo { id: "am_puck".into(), name: "Puck".into(), language: "en-US".into(), gender: "Male".into() },
        // British English - Female
        VoiceInfo { id: "bf_alice".into(), name: "Alice".into(), language: "en-GB".into(), gender: "Female".into() },
        VoiceInfo { id: "bf_emma".into(), name: "Emma".into(), language: "en-GB".into(), gender: "Female".into() },
        VoiceInfo { id: "bf_isabella".into(), name: "Isabella".into(), language: "en-GB".into(), gender: "Female".into() },
        VoiceInfo { id: "bf_lily".into(), name: "Lily".into(), language: "en-GB".into(), gender: "Female".into() },
        // British English - Male
        VoiceInfo { id: "bm_daniel".into(), name: "Daniel".into(), language: "en-GB".into(), gender: "Male".into() },
        VoiceInfo { id: "bm_fable".into(), name: "Fable (UK)".into(), language: "en-GB".into(), gender: "Male".into() },
        VoiceInfo { id: "bm_george".into(), name: "George".into(), language: "en-GB".into(), gender: "Male".into() },
        VoiceInfo { id: "bm_lewis".into(), name: "Lewis".into(), language: "en-GB".into(), gender: "Male".into() },
        // French
        VoiceInfo { id: "ff_siwis".into(), name: "Siwis".into(), language: "fr-FR".into(), gender: "Female".into() },
        // Hindi
        VoiceInfo { id: "hf_alpha".into(), name: "Alpha".into(), language: "hi-IN".into(), gender: "Female".into() },
        VoiceInfo { id: "hf_beta".into(), name: "Beta".into(), language: "hi-IN".into(), gender: "Female".into() },
        VoiceInfo { id: "hm_omega".into(), name: "Omega".into(), language: "hi-IN".into(), gender: "Male".into() },
        VoiceInfo { id: "hm_psi".into(), name: "Psi".into(), language: "hi-IN".into(), gender: "Male".into() },
        // Italian
        VoiceInfo { id: "if_sara".into(), name: "Sara".into(), language: "it-IT".into(), gender: "Female".into() },
        VoiceInfo { id: "im_nicola".into(), name: "Nicola".into(), language: "it-IT".into(), gender: "Male".into() },
        // Japanese
        VoiceInfo { id: "jf_alpha".into(), name: "Alpha".into(), language: "ja-JP".into(), gender: "Female".into() },
        VoiceInfo { id: "jf_gongitsune".into(), name: "Gongitsune".into(), language: "ja-JP".into(), gender: "Female".into() },
        VoiceInfo { id: "jf_nezumi".into(), name: "Nezumi".into(), language: "ja-JP".into(), gender: "Female".into() },
        VoiceInfo { id: "jf_tebukuro".into(), name: "Tebukuro".into(), language: "ja-JP".into(), gender: "Female".into() },
        VoiceInfo { id: "jm_kumo".into(), name: "Kumo".into(), language: "ja-JP".into(), gender: "Male".into() },
        // Korean
        VoiceInfo { id: "kf_alpha".into(), name: "Alpha".into(), language: "ko-KR".into(), gender: "Female".into() },
        VoiceInfo { id: "kf_beta".into(), name: "Beta".into(), language: "ko-KR".into(), gender: "Female".into() },
        // Portuguese (Brazil)
        VoiceInfo { id: "pf_dora".into(), name: "Dora".into(), language: "pt-BR".into(), gender: "Female".into() },
        VoiceInfo { id: "pm_alex".into(), name: "Alex".into(), language: "pt-BR".into(), gender: "Male".into() },
        VoiceInfo { id: "pm_santa".into(), name: "Santa".into(), language: "pt-BR".into(), gender: "Male".into() },
        // Mandarin Chinese
        VoiceInfo { id: "zf_xiaobei".into(), name: "Xiaobei".into(), language: "zh-CN".into(), gender: "Female".into() },
        VoiceInfo { id: "zf_xiaoni".into(), name: "Xiaoni".into(), language: "zh-CN".into(), gender: "Female".into() },
        VoiceInfo { id: "zf_xiaoxiao".into(), name: "Xiaoxiao".into(), language: "zh-CN".into(), gender: "Female".into() },
        VoiceInfo { id: "zf_xiaoyi".into(), name: "Xiaoyi".into(), language: "zh-CN".into(), gender: "Female".into() },
        VoiceInfo { id: "zm_yunjian".into(), name: "Yunjian".into(), language: "zh-CN".into(), gender: "Male".into() },
        VoiceInfo { id: "zm_yunxi".into(), name: "Yunxi".into(), language: "zh-CN".into(), gender: "Male".into() },
        VoiceInfo { id: "zm_yunxia".into(), name: "Yunxia".into(), language: "zh-CN".into(), gender: "Male".into() },
        VoiceInfo { id: "zm_yunyang".into(), name: "Yunyang".into(), language: "zh-CN".into(), gender: "Male".into() },
        // Spanish
        VoiceInfo { id: "ef_dalia".into(), name: "Dalia".into(), language: "es-ES".into(), gender: "Female".into() },
        VoiceInfo { id: "em_alex".into(), name: "Alex".into(), language: "es-ES".into(), gender: "Male".into() },
        VoiceInfo { id: "em_santa".into(), name: "Santa".into(), language: "es-ES".into(), gender: "Male".into() },
    ]
}
