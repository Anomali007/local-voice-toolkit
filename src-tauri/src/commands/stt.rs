use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex, OnceLock};

use crate::audio::capture::{AudioCapture, SilenceConfig};
use crate::commands::settings::get_settings;

// Global state for recording
static RECORDING_STATE: OnceLock<RecordingState> = OnceLock::new();

struct RecordingState {
    is_recording: AtomicBool,
    capture: Mutex<Option<AudioCapture>>,
}

fn get_recording_state() -> &'static RecordingState {
    RECORDING_STATE.get_or_init(|| RecordingState {
        is_recording: AtomicBool::new(false),
        capture: Mutex::new(None),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopRecordingResult {
    pub audio_data: Vec<f32>,
    pub silence_triggered: bool,
}

#[tauri::command]
pub async fn start_recording(app: tauri::AppHandle) -> Result<(), String> {
    use crate::commands::permissions::{self, MicAuthStatus};

    let state = get_recording_state();

    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    // Gate on microphone permission so we never record an empty buffer
    match permissions::microphone_status() {
        MicAuthStatus::Authorized | MicAuthStatus::Unknown => {}
        MicAuthStatus::NotDetermined => {
            tracing::info!("Microphone permission not determined - prompting");
            permissions::prompt_microphone_access();
            return Err(
                "Blah³ needs microphone access. Respond to the system dialog, then try again."
                    .to_string(),
            );
        }
        MicAuthStatus::Denied | MicAuthStatus::Restricted => {
            return Err(
                "Microphone access not granted. Enable Blah³ in System Settings → Privacy & Security → Microphone."
                    .to_string(),
            );
        }
    }

    tracing::info!("Starting audio recording...");

    // Load silence detection settings
    let settings = match get_settings() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to load settings for recording, using defaults: {}", e);
            crate::commands::settings::AppSettings::default()
        }
    };
    let silence_config = SilenceConfig {
        enabled: settings.silence_detection_enabled,
        threshold: settings.silence_threshold,
        duration_secs: settings.silence_duration,
    };

    let capture = AudioCapture::with_silence_config(silence_config)
        .map_err(|e| format!("Failed to initialize audio capture: {}", e))?;
    capture.start()
        .map_err(|e| format!("Failed to start microphone recording: {}", e))?;

    {
        let mut capture_guard = state.capture.lock()
            .map_err(|e| format!("Internal error: audio state lock poisoned: {}", e))?;
        *capture_guard = Some(capture);
    }

    state.is_recording.store(true, Ordering::SeqCst);
    tracing::info!("Recording started");

    // Emit real audio levels (~20 fps) so the panel waveform reflects the
    // actual microphone input instead of synthetic animation.
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let state = get_recording_state();
        loop {
            if !state.is_recording.load(Ordering::SeqCst) {
                break;
            }
            let level = match state.capture.lock() {
                Ok(guard) => guard.as_ref().map(|c| c.current_level()).unwrap_or(0.0),
                Err(_) => 0.0,
            };
            let _ = app.emit("stt-audio-level", level);
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_recording() -> Result<StopRecordingResult, String> {
    let state = get_recording_state();

    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    tracing::info!("Stopping audio recording...");

    let (audio_data, silence_triggered) = {
        let mut capture_guard = state.capture.lock()
            .map_err(|e| format!("Internal error: audio state lock poisoned: {}", e))?;
        if let Some(capture) = capture_guard.take() {
            let triggered = capture.is_silence_triggered();
            let data = capture.stop()
                .map_err(|e| format!("Failed to stop audio capture: {}", e))?;
            (data, triggered)
        } else {
            (Vec::new(), false)
        }
    };

    state.is_recording.store(false, Ordering::SeqCst);
    tracing::info!(
        "Recording stopped (silence_triggered: {}), captured {} samples",
        silence_triggered,
        audio_data.len()
    );

    Ok(StopRecordingResult {
        audio_data,
        silence_triggered,
    })
}

/// Check if recording was auto-stopped by silence detection.
/// Call this periodically from the frontend to detect auto-stop.
#[tauri::command]
pub fn is_silence_triggered() -> bool {
    let state = get_recording_state();
    let capture_guard = match state.capture.lock() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!("Failed to acquire audio state lock: {}", e);
            return false;
        }
    };

    if let Some(ref capture) = *capture_guard {
        capture.is_silence_triggered()
    } else {
        false
    }
}

/// Check if currently recording.
#[tauri::command]
pub fn is_recording() -> bool {
    let state = get_recording_state();
    state.is_recording.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn transcribe_audio(
    audio_data: Vec<f32>,
    model_path: String,
) -> Result<TranscriptionResult, String> {
    tracing::info!(
        "Transcribing {} samples with model: {}",
        audio_data.len(),
        model_path
    );

    if audio_data.is_empty() {
        if crate::commands::permissions::microphone_status()
            != crate::commands::permissions::MicAuthStatus::Authorized
        {
            return Err(
                "Microphone access not granted. Enable Blah³ in System Settings → Privacy & Security → Microphone."
                    .to_string(),
            );
        }
        return Err("No audio captured. Please check your microphone input device.".to_string());
    }

    let start = std::time::Instant::now();

    let engine = crate::engines::whisper::get_or_load_cached(&model_path)
        .map_err(|e| format!("Failed to load Whisper model '{}': {}", model_path, e))?;
    let text = engine.transcribe(&audio_data)
        .map_err(|e| format!("Transcription failed: {}", e))?;

    let duration_ms = start.elapsed().as_millis() as u64;
    tracing::info!("Transcription completed in {}ms: {}", duration_ms, text);

    Ok(TranscriptionResult { text, duration_ms })
}
