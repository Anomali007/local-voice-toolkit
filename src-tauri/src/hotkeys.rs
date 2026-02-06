use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::accessibility::{self, FrontmostAppInfo};
use crate::audio::capture::AudioCapture;
use crate::commands::settings::get_settings;
use crate::overlay;

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

/// Register all global hotkeys (internal - registers shortcuts and handlers)
fn register_hotkeys_internal(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let settings = match get_settings() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to load settings for hotkeys, using defaults: {}", e);
            crate::commands::settings::AppSettings::default()
        }
    };

    // Parse hotkeys from settings or use defaults
    let stt_shortcut = parse_shortcut(&settings.stt_hotkey)
        .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD));

    let tts_shortcut = parse_shortcut(&settings.tts_hotkey)
        .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS));

    tracing::info!("Registering STT hotkey: {:?}", stt_shortcut);
    tracing::info!("Registering TTS hotkey: {:?}", tts_shortcut);

    // on_shortcut both sets up the handler AND registers the shortcut
    app.global_shortcut().on_shortcut(stt_shortcut, move |app, shortcut, event| {
        handle_stt_shortcut(app, shortcut, event.state);
    })?;

    app.global_shortcut().on_shortcut(tts_shortcut, move |app, shortcut, event| {
        handle_tts_shortcut(app, shortcut, event.state);
    })?;

    Ok(())
}

/// Register all global hotkeys (called at startup)
pub fn register_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    register_hotkeys_internal(app)
}

/// Re-register hotkeys after settings change
pub fn refresh_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Unregister all existing shortcuts first
    if let Err(e) = app.global_shortcut().unregister_all() {
        tracing::warn!("Failed to unregister existing hotkeys: {}", e);
    }
    tracing::info!("Unregistered all hotkeys for refresh");

    // Re-register with new settings
    register_hotkeys_internal(app)?;

    tracing::info!("Hotkeys refreshed successfully");
    Ok(())
}

/// Payload for stt-recording-started event
#[derive(Clone, serde::Serialize)]
struct SttRecordingStartedPayload {
    target_app: Option<FrontmostAppInfo>,
}

/// Handle STT (dictation) shortcut - press to start, release to stop
fn handle_stt_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutState) {
    let state = app.state::<Arc<HotkeyState>>();

    match event {
        ShortcutState::Pressed => {
            if !state.is_recording.load(Ordering::SeqCst) {
                tracing::info!("STT hotkey pressed - starting recording");

                // Capture frontmost app BEFORE showing overlay
                let target_app = accessibility::get_frontmost_app();
                tracing::debug!("Target app for dictation: {:?}", target_app);

                state.is_recording.store(true, Ordering::SeqCst);

                // Show the dictation overlay
                if let Err(e) = overlay::show_overlay(app) {
                    tracing::warn!("Failed to show dictation overlay: {}", e);
                }

                // Emit event to frontend with target app info
                let payload = SttRecordingStartedPayload {
                    target_app: target_app.clone(),
                };
                if let Err(e) = app.emit("stt-recording-started", payload) {
                    tracing::warn!("Failed to emit stt-recording-started event: {}", e);
                }

                // Start audio capture in background
                let app_handle = app.clone();
                let state_clone = Arc::clone(&state);
                tauri::async_runtime::spawn(async move {
                    match AudioCapture::new() {
                        Ok(capture) => {
                            if let Err(e) = capture.start() {
                                tracing::error!("Failed to start audio capture: {}", e);
                                if let Err(emit_err) = app_handle.emit("stt-error", format!("Failed to start microphone: {}", e)) {
                                    tracing::warn!("Failed to emit error to UI: {}", emit_err);
                                }
                                // Hide overlay on error
                                let _ = overlay::hide_overlay(&app_handle);
                                return;
                            }
                            let mut guard = state_clone.audio_capture.lock().await;
                            *guard = Some(capture);

                            // Spawn audio level emission task for overlay visualization
                            let app_for_levels = app_handle.clone();
                            let state_for_levels = Arc::clone(&state_clone);
                            tauri::async_runtime::spawn(async move {
                                loop {
                                    if !state_for_levels.is_recording.load(Ordering::SeqCst) {
                                        break;
                                    }
                                    let level = {
                                        let guard = state_for_levels.audio_capture.lock().await;
                                        guard.as_ref().map(|c| c.current_level()).unwrap_or(0.0)
                                    };
                                    let _ = app_for_levels.emit("stt-audio-level", level);
                                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to create audio capture: {}", e);
                            if let Err(emit_err) = app_handle.emit("stt-error", format!("Microphone unavailable: {}", e)) {
                                tracing::warn!("Failed to emit error to UI: {}", emit_err);
                            }
                            // Hide overlay on error
                            let _ = overlay::hide_overlay(&app_handle);
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
                if let Err(e) = app.emit("stt-recording-stopped", ()) {
                    tracing::warn!("Failed to emit stt-recording-stopped event: {}", e);
                }

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
                                    if let Err(emit_err) = app_handle.emit("stt-error", format!("Recording error: {}", e)) {
                                        tracing::warn!("Failed to emit error to UI: {}", emit_err);
                                    }
                                    return;
                                }
                            }
                        } else {
                            Vec::new()
                        }
                    };

                    if audio_data.is_empty() {
                        tracing::warn!("No audio data captured");
                        if let Err(e) = app_handle.emit("stt-error", "No audio captured. Please check microphone permissions.") {
                            tracing::warn!("Failed to emit error to UI: {}", e);
                        }
                        // Hide overlay on error after brief delay
                        let app_for_hide = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            let _ = overlay::hide_overlay(&app_for_hide);
                        });
                        return;
                    }

                    tracing::info!("Captured {} audio samples, transcribing...", audio_data.len());
                    if let Err(e) = app_handle.emit("stt-transcribing", ()) {
                        tracing::warn!("Failed to emit stt-transcribing event: {}", e);
                    }

                    // Get model path from settings
                    let settings = match get_settings() {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!("Failed to load settings for transcription, using defaults: {}", e);
                            crate::commands::settings::AppSettings::default()
                        }
                    };
                    let models_dir = match dirs::data_dir() {
                        Some(dir) => dir.join("com.blahcubed.app").join("models").join("stt"),
                        None => {
                            tracing::error!("Could not determine data directory");
                            if let Err(e) = app_handle.emit("stt-error", "Could not find application data directory") {
                                tracing::warn!("Failed to emit error to UI: {}", e);
                            }
                            // Hide overlay on error after brief delay
                            let app_for_hide = app_handle.clone();
                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                                let _ = overlay::hide_overlay(&app_for_hide);
                            });
                            return;
                        }
                    };
                    let model_path = models_dir.join(&settings.stt_model);

                    if !model_path.exists() {
                        let error_msg = format!("Model not found: {}. Please download it from the Models tab.", settings.stt_model);
                        if let Err(e) = app_handle.emit("stt-error", &error_msg) {
                            tracing::warn!("Failed to emit error to UI: {}", e);
                        }
                        // Hide overlay on error after brief delay
                        let app_for_hide = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                            let _ = overlay::hide_overlay(&app_for_hide);
                        });
                        return;
                    }

                    // Transcribe - use to_string_lossy() to safely handle non-UTF8 paths
                    let model_path_str = model_path.to_string_lossy();
                    match crate::engines::whisper::WhisperEngine::new(&model_path_str) {
                        Ok(engine) => {
                            match engine.transcribe(&audio_data) {
                                Ok(text) => {
                                    tracing::info!("Transcription: {}", text);
                                    if let Err(e) = app_handle.emit("stt-result", &text) {
                                        tracing::warn!("Failed to emit transcription result: {}", e);
                                    }

                                    // Auto-paste if enabled
                                    if settings.auto_paste && !text.is_empty() {
                                        if let Err(e) = accessibility::paste_text(&text) {
                                            tracing::error!("Failed to auto-paste transcription: {}", e);
                                        }
                                    }

                                    // Hide overlay after a brief delay to show the result
                                    let app_for_hide = app_handle.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                                        let _ = overlay::hide_overlay(&app_for_hide);
                                    });
                                }
                                Err(e) => {
                                    tracing::error!("Transcription failed: {}", e);
                                    if let Err(emit_err) = app_handle.emit("stt-error", format!("Transcription failed: {}", e)) {
                                        tracing::warn!("Failed to emit error to UI: {}", emit_err);
                                    }
                                    // Hide overlay on error after brief delay
                                    let app_for_hide = app_handle.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                                        let _ = overlay::hide_overlay(&app_for_hide);
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to load Whisper model: {}", e);
                            if let Err(emit_err) = app_handle.emit("stt-error", format!("Failed to load speech model: {}", e)) {
                                tracing::warn!("Failed to emit error to UI: {}", emit_err);
                            }
                            // Hide overlay on error after brief delay
                            let app_for_hide = app_handle.clone();
                            tauri::async_runtime::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                                let _ = overlay::hide_overlay(&app_for_hide);
                            });
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
            tracing::warn!("No text selected for TTS");
            if let Err(e) = app.emit("tts-error", "No text selected. Please select some text first.") {
                tracing::warn!("Failed to emit tts-error event: {}", e);
            }
            return;
        }
    };

    tracing::info!("Selected text: {} chars", text.len());
    if let Err(e) = app.emit("tts-started", &text) {
        tracing::warn!("Failed to emit tts-started event: {}", e);
    }

    // Speak in background
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let settings = match get_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to load settings for TTS, using defaults: {}", e);
                crate::commands::settings::AppSettings::default()
            }
        };

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
        if let Err(e) = app_handle.emit("tts-finished", ()) {
            tracing::warn!("Failed to emit tts-finished event: {}", e);
        }
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
