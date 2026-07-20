use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::accessibility::{self, FrontmostAppInfo};
use crate::audio::capture::{AudioCapture, SilenceConfig};
use crate::commands::settings::get_settings;
use crate::overlay;

/// A key release faster than this is treated as a "tap" (toggle mode: keep
/// recording until the hotkey is pressed again, silence auto-stops, or Escape
/// cancels). A slower release is treated as push-to-talk (stop on release).
pub(crate) const HOLD_THRESHOLD_MS: u128 = 500;

/// Shared state for tracking recording status
pub struct HotkeyState {
    pub is_recording: AtomicBool,
    pub audio_capture: tokio::sync::Mutex<Option<AudioCapture>>,
    pub started_at: std::sync::Mutex<Option<Instant>>,
}

impl Default for HotkeyState {
    fn default() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            audio_capture: tokio::sync::Mutex::new(None),
            started_at: std::sync::Mutex::new(None),
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

    // The dictation trigger is either the bare right-Option key (native
    // event-tap monitor; accelerators can't express a lone modifier) or a
    // regular accelerator shortcut.
    let use_right_option = settings
        .stt_hotkey
        .trim()
        .eq_ignore_ascii_case(crate::right_option::RIGHT_OPTION_HOTKEY);
    crate::right_option::set_enabled(app, use_right_option);

    let tts_shortcut = parse_shortcut(&settings.tts_hotkey)
        .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS));
    tracing::info!("Registering TTS hotkey: {:?}", tts_shortcut);

    if !use_right_option {
        let stt_shortcut = parse_shortcut(&settings.stt_hotkey)
            .unwrap_or_else(|| Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD));
        tracing::info!("Registering STT hotkey: {:?}", stt_shortcut);

        // on_shortcut both sets up the handler AND registers the shortcut
        app.global_shortcut().on_shortcut(stt_shortcut, move |app, shortcut, event| {
            handle_stt_shortcut(app, shortcut, event.state);
        })?;
    } else {
        tracing::info!("STT trigger: right-Option key (native monitor)");
    }

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

/// Payload for stt-permission-error events (actionable permission toasts)
#[derive(Clone, serde::Serialize)]
struct SttPermissionErrorPayload {
    /// "microphone" | "accessibility"
    kind: String,
    message: String,
}

/// Emit a structured permission error the frontend can render with an
/// "Open Settings" action (falls back to a plain stt-error string too).
fn emit_permission_error(app: &AppHandle, kind: &str, message: &str) {
    let payload = SttPermissionErrorPayload {
        kind: kind.to_string(),
        message: message.to_string(),
    };
    if let Err(e) = app.emit("stt-permission-error", payload) {
        tracing::warn!("Failed to emit stt-permission-error event: {}", e);
        // Fallback so the user still sees something
        let _ = app.emit("stt-error", message);
    }
}

/// Handle STT (dictation) shortcut.
/// Hybrid model:
/// - Tap once to start recording, tap again to stop-and-transcribe.
/// - Or hold the hotkey push-to-talk style and release to stop.
/// - Escape cancels an in-progress recording without pasting anything.
/// - If silence detection is enabled, recording auto-stops after silence.
fn handle_stt_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutState) {
    let state = app.state::<Arc<HotkeyState>>();

    match event {
        ShortcutState::Pressed => {
            if state
                .is_recording
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                tracing::info!("STT hotkey pressed - starting recording");
                start_dictation(app);
            } else {
                // Second press while recording = toggle off
                tracing::info!("STT hotkey pressed while recording - stopping");
                stop_and_transcribe(app);
            }
        }
        ShortcutState::Released => {
            let held_ms = state
                .started_at
                .lock()
                .ok()
                .and_then(|guard| guard.map(|t| t.elapsed().as_millis()))
                .unwrap_or(0);

            if state.is_recording.load(Ordering::SeqCst) && held_ms >= HOLD_THRESHOLD_MS {
                // Push-to-talk: key was held down while speaking
                tracing::info!("STT hotkey released after {}ms hold - stopping recording", held_ms);
                stop_and_transcribe(app);
            }
            // Quick release (< threshold) = tap: keep recording (toggle mode)
        }
    }
}

/// Begin a dictation session: show overlay, start capture, arm Escape-to-cancel.
/// Caller must have already flipped `is_recording` false -> true.
pub(crate) fn start_dictation(app: &AppHandle) {
    let state = app.state::<Arc<HotkeyState>>();

    // Gate on microphone permission BEFORE opening the overlay so we never
    // record an empty buffer and fail with a cryptic transcription error.
    use crate::commands::permissions::{self, MicAuthStatus};
    match permissions::microphone_status() {
        MicAuthStatus::Authorized | MicAuthStatus::Unknown => {}
        MicAuthStatus::NotDetermined => {
            tracing::info!("Microphone permission not determined - prompting");
            permissions::prompt_microphone_access();
            emit_permission_error(
                app,
                "microphone",
                "Blah³ needs microphone access. Respond to the system dialog, then dictate again.",
            );
            state.is_recording.store(false, Ordering::SeqCst);
            return;
        }
        MicAuthStatus::Denied | MicAuthStatus::Restricted => {
            emit_permission_error(
                app,
                "microphone",
                "Microphone access not granted. Enable Blah³ in System Settings → Privacy & Security → Microphone.",
            );
            state.is_recording.store(false, Ordering::SeqCst);
            return;
        }
    }

    if let Ok(mut guard) = state.started_at.lock() {
        *guard = Some(Instant::now());
    }

    // Capture frontmost app BEFORE showing overlay
    let target_app = accessibility::get_frontmost_app();
    tracing::debug!("Target app for dictation: {:?}", target_app);

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

    // Escape cancels the recording while it is active
    register_cancel_shortcut(app);

    // Start audio capture in background
    let app_handle = app.clone();
    let state_clone = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let settings = match get_settings() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to load settings for capture, using defaults: {}", e);
                crate::commands::settings::AppSettings::default()
            }
        };
        let silence_config = SilenceConfig {
            enabled: settings.silence_detection_enabled,
            threshold: settings.silence_threshold,
            duration_secs: settings.silence_duration,
        };

        match AudioCapture::with_silence_config(silence_config) {
            Ok(capture) => {
                if let Err(e) = capture.start() {
                    tracing::error!("Failed to start audio capture: {}", e);
                    if let Err(emit_err) = app_handle.emit("stt-error", format!("Failed to start microphone: {}", e)) {
                        tracing::warn!("Failed to emit error to UI: {}", emit_err);
                    }
                    abort_dictation(&app_handle);
                    return;
                }
                let mut guard = state_clone.audio_capture.lock().await;
                *guard = Some(capture);
                drop(guard);

                // Spawn audio level emission + silence watcher for the overlay
                let app_for_levels = app_handle.clone();
                let state_for_levels = Arc::clone(&state_clone);
                tauri::async_runtime::spawn(async move {
                    loop {
                        if !state_for_levels.is_recording.load(Ordering::SeqCst) {
                            break;
                        }
                        let (level, silence_triggered) = {
                            let guard = state_for_levels.audio_capture.lock().await;
                            match guard.as_ref() {
                                Some(c) => (c.current_level(), c.is_silence_triggered()),
                                None => (0.0, false),
                            }
                        };
                        if silence_triggered {
                            tracing::info!("Silence detected - auto-stopping dictation");
                            stop_and_transcribe(&app_for_levels);
                            break;
                        }
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
                abort_dictation(&app_handle);
            }
        }
    });
}

/// Reset recording state and hide the overlay after a startup failure.
fn abort_dictation(app: &AppHandle) {
    let state = app.state::<Arc<HotkeyState>>();
    state.is_recording.store(false, Ordering::SeqCst);
    unregister_cancel_shortcut(app);
    let _ = overlay::hide_overlay(app);
}

/// Cancel an in-progress recording: discard audio, no transcription, no paste.
pub(crate) fn cancel_dictation(app: &AppHandle) {
    let state = app.state::<Arc<HotkeyState>>();
    if state
        .is_recording
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // Nothing to cancel
    }

    tracing::info!("Dictation cancelled");
    unregister_cancel_shortcut(app);

    if let Err(e) = app.emit("stt-cancelled", ()) {
        tracing::warn!("Failed to emit stt-cancelled event: {}", e);
    }
    let _ = overlay::hide_overlay(app);

    let state_clone = Arc::clone(&state);
    tauri::async_runtime::spawn(async move {
        let mut guard = state_clone.audio_capture.lock().await;
        if let Some(capture) = guard.take() {
            let _ = capture.stop();
        }
    });
}

/// Stop the current recording and run transcription + auto-paste.
/// Safe to call from multiple triggers (toggle press, hold release, silence
/// auto-stop) - only the first caller proceeds.
pub(crate) fn stop_and_transcribe(app: &AppHandle) {
    let state = app.state::<Arc<HotkeyState>>();
    if state
        .is_recording
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return; // Already stopped or cancelled
    }

    unregister_cancel_shortcut(app);

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
                        hide_overlay_after_delay(&app_handle);
                        return;
                    }
                }
            } else {
                Vec::new()
            }
        };

        if audio_data.is_empty() {
            tracing::warn!("No audio data captured");
            if crate::commands::permissions::microphone_status()
                != crate::commands::permissions::MicAuthStatus::Authorized
            {
                emit_permission_error(
                    &app_handle,
                    "microphone",
                    "Microphone access not granted. Enable Blah³ in System Settings → Privacy & Security → Microphone.",
                );
            } else if let Err(e) = app_handle.emit("stt-error", "No audio captured. Please check your microphone input device.") {
                tracing::warn!("Failed to emit error to UI: {}", e);
            }
            hide_overlay_after_delay(&app_handle);
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
                hide_overlay_after_delay(&app_handle);
                return;
            }
        };
        let model_path = models_dir.join(&settings.stt_model);

        if !model_path.exists() {
            let error_msg = format!("Model not found: {}. Please download it from the Models tab.", settings.stt_model);
            if let Err(e) = app_handle.emit("stt-error", &error_msg) {
                tracing::warn!("Failed to emit error to UI: {}", e);
            }
            hide_overlay_after_delay(&app_handle);
            return;
        }

        // Transcribe - use to_string_lossy() to safely handle non-UTF8 paths.
        // The engine is cached across dictations to avoid reloading the model.
        let model_path_str = model_path.to_string_lossy();
        match crate::engines::whisper::get_or_load_cached(&model_path_str) {
            Ok(engine) => {
                let app_for_segments = app_handle.clone();
                let mut accumulated_text = String::new();
                let on_segment = move |data: whisper_rs::SegmentCallbackData| {
                    accumulated_text.push_str(&data.text);
                    let _ = app_for_segments.emit("stt-partial-result", accumulated_text.trim());
                };
                match engine.transcribe_streaming(&audio_data, on_segment) {
                    Ok(text) => {
                        tracing::info!("Transcription: {}", text);
                        if let Err(e) = app_handle.emit("stt-result", &text) {
                            tracing::warn!("Failed to emit transcription result: {}", e);
                        }

                        // Auto-paste if enabled
                        if settings.auto_paste && !text.is_empty() {
                            if let Err(e) = accessibility::paste_text(&text) {
                                tracing::error!("Failed to auto-paste transcription: {}", e);
                                // Offer the system Accessibility dialog once
                                crate::commands::permissions::prompt_accessibility_once();
                                emit_permission_error(
                                    &app_handle,
                                    "accessibility",
                                    "Auto-paste failed - the text is in your clipboard, press Cmd+V to paste. Grant Blah³ Accessibility (and Automation) permission for auto-paste.",
                                );
                            }
                        }

                        hide_overlay_after_delay(&app_handle);
                    }
                    Err(e) => {
                        tracing::error!("Transcription failed: {}", e);
                        if let Err(emit_err) = app_handle.emit("stt-error", format!("Transcription failed: {}", e)) {
                            tracing::warn!("Failed to emit error to UI: {}", emit_err);
                        }
                        hide_overlay_after_delay(&app_handle);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to load Whisper model: {}", e);
                if let Err(emit_err) = app_handle.emit("stt-error", format!("Failed to load speech model: {}", e)) {
                    tracing::warn!("Failed to emit error to UI: {}", emit_err);
                }
                hide_overlay_after_delay(&app_handle);
            }
        }
    });
}

/// Hide the dictation overlay after a brief delay (so results/errors stay visible).
fn hide_overlay_after_delay(app: &AppHandle) {
    let app_for_hide = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let _ = overlay::hide_overlay(&app_for_hide);
    });
}

fn cancel_shortcut() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

/// Register a global Escape shortcut for the duration of a recording so the
/// user can bail out without transcribing/pasting.
fn register_cancel_shortcut(app: &AppHandle) {
    let result = app
        .global_shortcut()
        .on_shortcut(cancel_shortcut(), |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                cancel_dictation(app);
            }
        });
    if let Err(e) = result {
        // Non-fatal: recording still works, just no Escape-to-cancel
        tracing::warn!("Failed to register Escape cancel shortcut: {}", e);
    }
}

/// Release the global Escape shortcut once recording ends.
fn unregister_cancel_shortcut(app: &AppHandle) {
    if let Err(e) = app.global_shortcut().unregister(cancel_shortcut()) {
        tracing::debug!("Failed to unregister Escape cancel shortcut: {}", e);
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

        let model_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.blahcubed.app")
            .join("models")
            .join("tts")
            .join("kokoro-v1.0.onnx")
            .to_string_lossy()
            .to_string();

        match crate::commands::tts::speak_text(
            text.clone(),
            settings.tts_voice.clone(),
            settings.tts_speed,
            model_path,
        ).await {
            Ok(()) => {
                tracing::info!("TTS playback completed");
                if let Err(e) = app_handle.emit("tts-finished", ()) {
                    tracing::warn!("Failed to emit tts-finished event: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("TTS failed: {}", e);
                if let Err(emit_err) = app_handle.emit("tts-error", format!("Speech failed: {}", e)) {
                    tracing::warn!("Failed to emit tts-error event: {}", emit_err);
                }
            }
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
