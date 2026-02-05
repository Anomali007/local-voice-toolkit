use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex;

use crate::audio::capture::AudioCapture;
use crate::engines::whisper::WhisperEngine;

#[derive(Default)]
pub struct SttState {
    pub is_recording: AtomicBool,
    pub audio_buffer: Mutex<Vec<f32>>,
    pub capture: Mutex<Option<AudioCapture>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub duration_ms: u64,
}

#[tauri::command]
pub async fn start_recording(state: State<'_, Arc<SttState>>) -> Result<(), String> {
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    tracing::info!("Starting audio recording...");

    let mut capture_guard = state.capture.lock().await;
    let mut buffer_guard = state.audio_buffer.lock().await;
    buffer_guard.clear();

    let capture = AudioCapture::new().map_err(|e| e.to_string())?;
    capture.start().map_err(|e| e.to_string())?;
    *capture_guard = Some(capture);

    state.is_recording.store(true, Ordering::SeqCst);
    tracing::info!("Recording started");

    Ok(())
}

#[tauri::command]
pub async fn stop_recording(state: State<'_, Arc<SttState>>) -> Result<Vec<f32>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    tracing::info!("Stopping audio recording...");

    let mut capture_guard = state.capture.lock().await;
    let audio_data = if let Some(capture) = capture_guard.take() {
        capture.stop().map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };

    state.is_recording.store(false, Ordering::SeqCst);
    tracing::info!("Recording stopped, captured {} samples", audio_data.len());

    Ok(audio_data)
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
