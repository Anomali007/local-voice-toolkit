use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, Ordering}, Mutex, OnceLock};

use crate::audio::capture::{AudioCapture, SilenceConfig};
use crate::commands::settings::get_settings;
use crate::engines::whisper::WhisperEngine;

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
pub async fn start_recording() -> Result<(), String> {
    let state = get_recording_state();

    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    tracing::info!("Starting audio recording...");

    // Load silence detection settings
    let settings = get_settings().unwrap_or_default();
    let silence_config = SilenceConfig {
        enabled: settings.silence_detection_enabled,
        threshold: settings.silence_threshold,
        duration_secs: settings.silence_duration,
    };

    let capture = AudioCapture::with_silence_config(silence_config).map_err(|e| e.to_string())?;
    capture.start().map_err(|e| e.to_string())?;

    {
        let mut capture_guard = state.capture.lock().unwrap();
        *capture_guard = Some(capture);
    }

    state.is_recording.store(true, Ordering::SeqCst);
    tracing::info!("Recording started");

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
        let mut capture_guard = state.capture.lock().unwrap();
        if let Some(capture) = capture_guard.take() {
            let triggered = capture.is_silence_triggered();
            let data = capture.stop().map_err(|e| e.to_string())?;
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
    let capture_guard = state.capture.lock().unwrap();

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

    let start = std::time::Instant::now();

    let engine = WhisperEngine::new(&model_path).map_err(|e| e.to_string())?;
    let text = engine.transcribe(&audio_data).map_err(|e| e.to_string())?;

    let duration_ms = start.elapsed().as_millis() as u64;
    tracing::info!("Transcription completed in {}ms: {}", duration_ms, text);

    Ok(TranscriptionResult { text, duration_ms })
}
