use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::accessibility;
use crate::audio::capture::AudioCapture;
use crate::commands::settings::get_settings;

/// Shared state for tracking recording status
pub struct HotkeyState {
    pub is_recording: AtomicBool,
    pub audio_capture: tokio::sync::Mutex<Option<AudioCapture>>,
}

impl Default for HotkeyState {
    fn default() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            audio_capture: tokio::sync::Mutex::new(None),
        }
    }
}

/// Register all global hotkeys
pub fn register_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let settings = get_settings().unwrap_or_default();

    // Parse hotkeys from settings or use defaults
    let stt_shortcut = parse_shortcut(&settings.stt_hotkey)
        .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD));

    let tts_shortcut = parse_shortcut(&settings.tts_hotkey)
        .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS));

    tracing::info!("Registering STT hotkey: {:?}", stt_shortcut);
    tracing::info!("Registering TTS hotkey: {:?}", tts_shortcut);

    app.global_shortcut().on_shortcut(stt_shortcut, move |app, shortcut, event| {
        handle_stt_shortcut(app, shortcut, event.state);
    })?;

    app.global_shortcut().on_shortcut(tts_shortcut, move |app, shortcut, event| {
        handle_tts_shortcut(app, shortcut, event.state);
    })?;

    app.global_shortcut().register(stt_shortcut)?;
    app.global_shortcut().register(tts_shortcut)?;

    Ok(())
}

/// Handle STT (dictation) shortcut - press to start, release to stop
fn handle_stt_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutState) {
    let state = app.state::<Arc<HotkeyState>>();

    match event {
        ShortcutState::Pressed => {
            if !state.is_recording.load(Ordering::SeqCst) {
                tracing::info!("STT hotkey pressed - starting recording");
                state.is_recording.store(true, Ordering::SeqCst);

                // Emit event to frontend
                let _ = app.emit("stt-recording-started", ());

                // Start audio capture in background
                let app_handle = app.clone();
                let state_clone = Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    match AudioCapture::new() {
                        Ok(capture) => {
                            if let Err(e) = capture.start() {
                                tracing::error!("Failed to start audio capture: {}", e);
                                let _ = app_handle.emit("stt-error", e.to_string());
                                return;
                            }
                            let mut guard = state_clone.audio_capture.lock().await;
                            *guard = Some(capture);
                        }
                        Err(e) => {
                            tracing::error!("Failed to create audio capture: {}", e);
                            let _ = app_handle.emit("stt-error", e.to_string());
                        }
                    }
                });
            }
        }
        ShortcutState::Released => {
            if state.is_recording.load(Ordering::SeqCst) {
                tracing::info!("STT hotkey released - stopping recording");
                state.is_recording.store(false, Ordering::SeqCst);

                // Emit event to frontend
                let _ = app.emit("stt-recording-stopped", ());

                // Stop capture and transcribe in background
                let app_handle = app.clone();
                let state_clone = Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    let audio_data = {
                        let mut guard = state_clone.audio_capture.lock().await;
                        if let Some(capture) = guard.take() {
                            match capture.stop() {
                                Ok(data) => data,
                                Err(e) => {
                                    tracing::error!("Failed to stop capture: {}", e);
                                    let _ = app_handle.emit("stt-error", e.to_string());
                                    return;
                                }
                            }
                        } else {
                            Vec::new()
                        }
                    };

                    if audio_data.is_empty() {
                        tracing::warn!("No audio data captured");
                        let _ = app_handle.emit("stt-error", "No audio captured");
                        return;
                    }

                    tracing::info!("Captured {} audio samples, transcribing...", audio_data.len());
                    let _ = app_handle.emit("stt-transcribing", ());

                    // Get model path from settings
                    let settings = get_settings().unwrap_or_default();
                    let models_dir = dirs::data_dir()
                        .unwrap_or_default()
                        .join("com.blahcubed.app")
                        .join("models")
                        .join("stt");
                    let model_path = models_dir.join(&settings.stt_model);

                    if !model_path.exists() {
                        let _ = app_handle.emit("stt-error", format!("Model not found: {}. Please download it first.", settings.stt_model));
                        return;
                    }

                    // Transcribe
                    match crate::engines::whisper::WhisperEngine::new(model_path.to_str().unwrap()) {
                        Ok(engine) => {
                            match engine.transcribe(&audio_data) {
                                Ok(text) => {
                                    tracing::info!("Transcription: {}", text);
                                    let _ = app_handle.emit("stt-result", &text);

                                    // Auto-paste if enabled
                                    if settings.auto_paste && !text.is_empty() {
                                        if let Err(e) = accessibility::paste_text(&text) {
                                            tracing::error!("Failed to paste: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Transcription failed: {}", e);
                                    let _ = app_handle.emit("stt-error", e.to_string());
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to load Whisper model: {}", e);
                            let _ = app_handle.emit("stt-error", e.to_string());
                        }
                    }
                });
            }
        }
    }
}

/// Handle TTS (read aloud) shortcut - single press to read selection
fn handle_tts_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutState) {
    if event != ShortcutState::Pressed {
        return;
    }

    tracing::info!("TTS hotkey pressed - reading selection");

    // Get selected text
    let text = match accessibility::get_selected_text() {
        Some(t) if !t.is_empty() => t,
        _ => {
            tracing::warn!("No text selected");
            let _ = app.emit("tts-error", "No text selected");
            return;
        }
    };

    tracing::info!("Selected text: {} chars", text.len());
    let _ = app.emit("tts-started", &text);

    // Speak in background
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let settings = get_settings().unwrap_or_default();

        // For now, emit that we would speak the text
        // Full TTS integration requires kokoroxide
        tracing::info!("Would speak with voice '{}' at {}x speed: {}",
            settings.tts_voice, settings.tts_speed, &text);

        // TODO: Implement actual TTS when kokoroxide is integrated
        // let models_dir = dirs::data_dir()
        //     .unwrap_or_default()
        //     .join("com.blahcubed.app")
        //     .join("models")
        //     .join("tts");
        // let model_path = models_dir.join("kokoro-v1.0.onnx");

        // Emit completion for now
        let _ = app_handle.emit("tts-finished", ());
    });
}

/// Parse a shortcut string like "CommandOrControl+Shift+D" into a Shortcut
fn parse_shortcut(shortcut_str: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = shortcut_str.split('+').collect();
    if parts.is_empty() {
        return None;
    }

    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for part in parts {
        let part = part.trim();
        match part.to_lowercase().as_str() {
            "command" | "commandorcontrol" | "cmd" | "super" => {
                modifiers |= Modifiers::SUPER;
            }
            "control" | "ctrl" => {
                modifiers |= Modifiers::CONTROL;
            }
            "shift" => {
                modifiers |= Modifiers::SHIFT;
            }
            "alt" | "option" => {
                modifiers |= Modifiers::ALT;
            }
            // Letters
            "a" => code = Some(Code::KeyA),
            "b" => code = Some(Code::KeyB),
            "c" => code = Some(Code::KeyC),
            "d" => code = Some(Code::KeyD),
            "e" => code = Some(Code::KeyE),
            "f" => code = Some(Code::KeyF),
            "g" => code = Some(Code::KeyG),
            "h" => code = Some(Code::KeyH),
            "i" => code = Some(Code::KeyI),
            "j" => code = Some(Code::KeyJ),
            "k" => code = Some(Code::KeyK),
            "l" => code = Some(Code::KeyL),
            "m" => code = Some(Code::KeyM),
            "n" => code = Some(Code::KeyN),
            "o" => code = Some(Code::KeyO),
            "p" => code = Some(Code::KeyP),
            "q" => code = Some(Code::KeyQ),
            "r" => code = Some(Code::KeyR),
            "s" => code = Some(Code::KeyS),
            "t" => code = Some(Code::KeyT),
            "u" => code = Some(Code::KeyU),
            "v" => code = Some(Code::KeyV),
            "w" => code = Some(Code::KeyW),
            "x" => code = Some(Code::KeyX),
            "y" => code = Some(Code::KeyY),
            "z" => code = Some(Code::KeyZ),
            // Numbers
            "0" => code = Some(Code::Digit0),
            "1" => code = Some(Code::Digit1),
            "2" => code = Some(Code::Digit2),
            "3" => code = Some(Code::Digit3),
            "4" => code = Some(Code::Digit4),
            "5" => code = Some(Code::Digit5),
            "6" => code = Some(Code::Digit6),
            "7" => code = Some(Code::Digit7),
            "8" => code = Some(Code::Digit8),
            "9" => code = Some(Code::Digit9),
            // Function keys
            "f1" => code = Some(Code::F1),
            "f2" => code = Some(Code::F2),
            "f3" => code = Some(Code::F3),
            "f4" => code = Some(Code::F4),
            "f5" => code = Some(Code::F5),
            "f6" => code = Some(Code::F6),
            "f7" => code = Some(Code::F7),
            "f8" => code = Some(Code::F8),
            "f9" => code = Some(Code::F9),
            "f10" => code = Some(Code::F10),
            "f11" => code = Some(Code::F11),
            "f12" => code = Some(Code::F12),
            // Special keys
            "space" => code = Some(Code::Space),
            "enter" | "return" => code = Some(Code::Enter),
            "escape" | "esc" => code = Some(Code::Escape),
            "tab" => code = Some(Code::Tab),
            "backspace" => code = Some(Code::Backspace),
            _ => {}
        }
    }

    code.map(|c| {
        if modifiers.is_empty() {
            Shortcut::new(None, c)
        } else {
            Shortcut::new(Some(modifiers), c)
        }
    })
}
