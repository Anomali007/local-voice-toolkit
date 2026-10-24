use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
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

/// Register all global hotkeys. Each shortcut is registered independently so
/// one conflicting/invalid hotkey doesn't prevent the others from working.
/// Returns the list of human-readable failures (empty on full success).
fn register_hotkeys_internal(app: &AppHandle) -> Vec<String> {
    let mut failures = Vec::new();
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

    if use_right_option {
        tracing::info!("STT trigger: right-Option key (native monitor)");
    } else {
        match parse_shortcut(&settings.stt_hotkey) {
            Some(stt_shortcut) => {
                tracing::info!("Registering STT hotkey: {:?}", stt_shortcut);
                // on_shortcut both sets up the handler AND registers the shortcut
                if let Err(e) = app.global_shortcut().on_shortcut(stt_shortcut, move |app, shortcut, event| {
                    handle_stt_shortcut(app, shortcut, event.state);
                }) {
                    tracing::error!("Failed to register STT hotkey '{}': {}", settings.stt_hotkey, e);
                    failures.push(format!("Dictation hotkey '{}' could not be registered: {}", settings.stt_hotkey, e));
                }
            }
            None => {
                tracing::error!("Invalid STT hotkey '{}'", settings.stt_hotkey);
                failures.push(format!("Dictation hotkey '{}' is not valid", settings.stt_hotkey));
            }
        }
    }

    match parse_shortcut(&settings.tts_hotkey) {
        Some(tts_shortcut) => {
            tracing::info!("Registering TTS hotkey: {:?}", tts_shortcut);
            if let Err(e) = app.global_shortcut().on_shortcut(tts_shortcut, move |app, shortcut, event| {
                handle_tts_shortcut(app, shortcut, event.state);
            }) {
                tracing::error!("Failed to register TTS hotkey '{}': {}", settings.tts_hotkey, e);
                failures.push(format!("Read aloud hotkey '{}' could not be registered: {}", settings.tts_hotkey, e));
            }
        }
        None => {
            tracing::error!("Invalid TTS hotkey '{}'", settings.tts_hotkey);
            failures.push(format!("Read aloud hotkey '{}' is not valid", settings.tts_hotkey));
        }
    }

    failures
}

/// Register all global hotkeys (called at startup). Failures are logged, not fatal.
pub fn register_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let failures = register_hotkeys_internal(app);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; ").into())
    }
}

/// Re-register hotkeys after a hotkey setting changed.
pub fn refresh_hotkeys(app: &AppHandle) -> Result<(), String> {
    // Unregister all existing shortcuts first
    if let Err(e) = app.global_shortcut().unregister_all() {
        tracing::warn!("Failed to unregister existing hotkeys: {}", e);
    }
    tracing::info!("Unregistered all hotkeys for refresh");

    let failures = register_hotkeys_internal(app);

    // unregister_all also dropped the Escape-cancel shortcut; restore it if a
    // recording is in flight.
    if app.state::<Arc<HotkeyState>>().is_recording.load(Ordering::SeqCst)
        || crate::commands::tts::is_speaking()
    {
        register_cancel_shortcut(app);
    }

    if failures.is_empty() {
        tracing::info!("Hotkeys refreshed successfully");
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Validate a hotkey string from settings. `allow_right_option` permits the
/// bare right-Option trigger (dictation only).
pub fn validate_hotkey(label: &str, hotkey: &str, allow_right_option: bool) -> Result<(), String> {
    let trimmed = hotkey.trim();
    if trimmed.is_empty() {
        return Err(format!("{} is not set", label));
    }
    if trimmed.eq_ignore_ascii_case(crate::right_option::RIGHT_OPTION_HOTKEY) {
        return if allow_right_option {
            Ok(())
        } else {
            Err(format!("{} cannot be the Right Option key alone", label))
        };
    }
    if parse_shortcut(trimmed).is_none() {
        return Err(format!("{} '{}' is not a valid shortcut", label, trimmed));
    }
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

                spawn_live_transcript(app_handle.clone(), Arc::clone(&state_clone), settings.stt_model.clone());
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
                    let _ = app_for_segments.emit("stt-partial-result", crate::engines::whisper::strip_non_speech(&accumulated_text));
                };
                match engine.transcribe_streaming(&audio_data, on_segment) {
                    Ok(text) => {
                        // Length only: transcripts are private and the log is a plain file.
                        tracing::info!("Transcription: {} chars", text.chars().count());
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
    let generation = overlay::overlay_generation();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        // A newer dictation re-showed the overlay while we slept: leave it alone.
        if overlay::overlay_generation() == generation {
            let _ = overlay::hide_overlay(&app_for_hide);
        }
    });
}

fn cancel_shortcut() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

/// Register a global Escape shortcut for the duration of a recording so the
/// user can bail out without transcribing/pasting.
fn register_cancel_shortcut(app: &AppHandle) {
    cancel_shortcut_worker(app, true);
}

/// Release the global Escape shortcut once recording ends.
fn unregister_cancel_shortcut(app: &AppHandle) {
    // Escape also stops a reading in progress; keep it while one is running.
    if crate::commands::tts::is_speaking() {
        return;
    }
    cancel_shortcut_worker(app, false);
}

/// Arm Escape-to-stop for a read-aloud session. Goes through the worker, so
/// it is safe from any context except it must never block on the plugin lock.
pub(crate) fn arm_read_escape(app: &AppHandle) {
    register_cancel_shortcut(app);
}

/// Release Escape after a reading, unless a dictation still needs it.
pub(crate) fn disarm_read_escape(app: &AppHandle) {
    if !app.state::<Arc<HotkeyState>>().is_recording.load(Ordering::SeqCst) {
        cancel_shortcut_worker(app, false);
    }
}

/// The global-shortcut plugin runs handlers while holding its shortcut-table
/// lock, and (un)registering takes the same lock. Doing it from inside a
/// handler - pressing Escape cancels, which unregisters Escape - deadlocked
/// the main thread and froze the overlay. A worker thread applies the
/// changes in order once the handler has returned.
fn cancel_shortcut_worker(app: &AppHandle, register: bool) {
    static WORKER: OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<bool>>> = OnceLock::new();
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let app = app.clone();
        std::thread::Builder::new()
            .name("cancel-shortcut".into())
            .spawn(move || {
                for register in rx {
                    if register {
                        let result = app.global_shortcut().on_shortcut(cancel_shortcut(), |app, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                // Only flags/emits here: (un)registering Escape
                                // from a handler deadlocks, the worker does it.
                                cancel_dictation(app);
                                crate::commands::tts::stop_reading();
                            }
                        });
                        if let Err(e) = result {
                            // Non-fatal: recording still works, just no Escape-to-cancel
                            tracing::warn!("Failed to register Escape cancel shortcut: {}", e);
                        }
                    } else if let Err(e) = app.global_shortcut().unregister(cancel_shortcut()) {
                        tracing::debug!("Failed to unregister Escape cancel shortcut: {}", e);
                    }
                }
            })
            .expect("spawn cancel-shortcut worker");
        std::sync::Mutex::new(tx)
    });
    if let Ok(tx) = sender.lock() {
        let _ = tx.send(register);
    }
}

/// Bumped per dictation so a live-transcript loop from an earlier session
/// stops instead of overwriting the new one's preview.
static LIVE_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Seconds of trailing audio the live preview transcribes. Whisper works in
/// 30 s windows; the final transcription after stop uses the whole recording.
const LIVE_WINDOW_SECS: usize = 25;

/// Model used for the live preview when a larger one is selected.
const LIVE_PREVIEW_MODEL: &str = "ggml-base.en.bin";

/// While recording, re-transcribe the audio so far every ~0.7 s and send it
/// to the overlay as `stt-partial-result`, so words appear while speaking.
fn spawn_live_transcript(app: AppHandle, state: Arc<HotkeyState>, stt_model: String) {
    let session = LIVE_SESSION.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn(async move {
        // The preview re-runs every 0.7 s, so prefer Base when a larger model
        // is selected; the final transcription still uses the selected model.
        let stt_dir = dirs::data_dir().map(|d| d.join("com.blahcubed.app").join("models").join("stt"));
        let Some(model_path) = stt_dir.and_then(|dir| {
            let fast = dir.join(LIVE_PREVIEW_MODEL);
            let selected = dir.join(&stt_model);
            let larger = !matches!(stt_model.as_str(), "ggml-tiny.en.bin" | "ggml-base.en.bin");
            if larger && fast.exists() {
                Some(fast)
            } else {
                Some(selected).filter(|p| p.exists())
            }
        }) else {
            return;
        };
        let path = model_path.to_string_lossy().to_string();
        let engine = match tokio::task::spawn_blocking(move || {
            crate::engines::whisper::get_or_load_cached(&path)
        })
        .await
        {
            Ok(Ok(engine)) => engine,
            _ => return, // The final transcription reports model errors.
        };

        let current = |state: &HotkeyState| {
            state.is_recording.load(Ordering::SeqCst) && LIVE_SESSION.load(Ordering::SeqCst) == session
        };
        let mut transcribed_len = 0usize;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(700)).await;
            if !current(&state) {
                break;
            }
            let audio = {
                let guard = state.audio_capture.lock().await;
                match guard.as_ref() {
                    Some(capture) => capture.snapshot(),
                    None => continue,
                }
            };
            // Wait for at least 1 s of audio and ~0.4 s of new audio.
            if audio.len() < 16_000 || audio.len() < transcribed_len + 6_400 {
                continue;
            }
            transcribed_len = audio.len();
            let window = LIVE_WINDOW_SECS * 16_000;
            let truncated = audio.len() > window;
            let tail = if truncated { audio[audio.len() - window..].to_vec() } else { audio };
            let engine = Arc::clone(&engine);
            let text = match tokio::task::spawn_blocking(move || engine.transcribe(&tail)).await {
                Ok(Ok(text)) => text,
                _ => continue,
            };
            if text.is_empty() || !current(&state) {
                continue;
            }
            let shown = if truncated { format!("… {}", text) } else { text };
            let _ = app.emit("stt-partial-result", shown);
        }
    });
}

/// Handle TTS (read aloud) shortcut: press to read the selection, press
/// again while it is reading to stop.
fn handle_tts_shortcut(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutState) {
    if event != ShortcutState::Pressed {
        return;
    }
    toggle_read_selection(app);
}

/// Read the current selection aloud, or stop if a reading is in progress.
/// Shared by the read-aloud hotkey and the menu bar item.
pub fn toggle_read_selection(app: &AppHandle) {
    if crate::commands::tts::is_speaking() {
        tracing::info!("TTS hotkey pressed while reading - stopping");
        // The reading task sees the stop and emits tts-finished itself.
        tauri::async_runtime::spawn(crate::commands::tts::stop_speaking());
        return;
    }

    tracing::info!("TTS hotkey pressed - reading selection");

    // Get selected text
    let text = match accessibility::get_selected_text() {
        Some(t) if !t.trim().is_empty() => t,
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

        match crate::commands::tts::speak_text_impl(
            Some(app_handle.clone()),
            text,
            settings.tts_voice.clone(),
            settings.tts_speed,
            Some(settings.tts_engine.clone()),
        ).await {
            Ok(()) => {
                tracing::info!("TTS playback finished");
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
/// Returns None for empty input, unknown tokens, or more than one non-modifier key.
pub(crate) fn parse_shortcut(shortcut_str: &str) -> Option<Shortcut> {
    if shortcut_str.trim().is_empty() {
        return None;
    }
    let parts: Vec<&str> = shortcut_str.split('+').collect();

    let mut modifiers = Modifiers::empty();
    let mut code: Option<Code> = None;
    // Assigns the key code; a second non-modifier key makes the shortcut invalid.
    fn set_key(slot: &mut Option<Code>, c: Code) -> Option<()> {
        if slot.is_some() {
            return None;
        }
        *slot = Some(c);
        Some(())
    }

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
            "a" => set_key(&mut code, Code::KeyA)?,
            "b" => set_key(&mut code, Code::KeyB)?,
            "c" => set_key(&mut code, Code::KeyC)?,
            "d" => set_key(&mut code, Code::KeyD)?,
            "e" => set_key(&mut code, Code::KeyE)?,
            "f" => set_key(&mut code, Code::KeyF)?,
            "g" => set_key(&mut code, Code::KeyG)?,
            "h" => set_key(&mut code, Code::KeyH)?,
            "i" => set_key(&mut code, Code::KeyI)?,
            "j" => set_key(&mut code, Code::KeyJ)?,
            "k" => set_key(&mut code, Code::KeyK)?,
            "l" => set_key(&mut code, Code::KeyL)?,
            "m" => set_key(&mut code, Code::KeyM)?,
            "n" => set_key(&mut code, Code::KeyN)?,
            "o" => set_key(&mut code, Code::KeyO)?,
            "p" => set_key(&mut code, Code::KeyP)?,
            "q" => set_key(&mut code, Code::KeyQ)?,
            "r" => set_key(&mut code, Code::KeyR)?,
            "s" => set_key(&mut code, Code::KeyS)?,
            "t" => set_key(&mut code, Code::KeyT)?,
            "u" => set_key(&mut code, Code::KeyU)?,
            "v" => set_key(&mut code, Code::KeyV)?,
            "w" => set_key(&mut code, Code::KeyW)?,
            "x" => set_key(&mut code, Code::KeyX)?,
            "y" => set_key(&mut code, Code::KeyY)?,
            "z" => set_key(&mut code, Code::KeyZ)?,
            // Numbers
            "0" => set_key(&mut code, Code::Digit0)?,
            "1" => set_key(&mut code, Code::Digit1)?,
            "2" => set_key(&mut code, Code::Digit2)?,
            "3" => set_key(&mut code, Code::Digit3)?,
            "4" => set_key(&mut code, Code::Digit4)?,
            "5" => set_key(&mut code, Code::Digit5)?,
            "6" => set_key(&mut code, Code::Digit6)?,
            "7" => set_key(&mut code, Code::Digit7)?,
            "8" => set_key(&mut code, Code::Digit8)?,
            "9" => set_key(&mut code, Code::Digit9)?,
            // Function keys
            "f1" => set_key(&mut code, Code::F1)?,
            "f2" => set_key(&mut code, Code::F2)?,
            "f3" => set_key(&mut code, Code::F3)?,
            "f4" => set_key(&mut code, Code::F4)?,
            "f5" => set_key(&mut code, Code::F5)?,
            "f6" => set_key(&mut code, Code::F6)?,
            "f7" => set_key(&mut code, Code::F7)?,
            "f8" => set_key(&mut code, Code::F8)?,
            "f9" => set_key(&mut code, Code::F9)?,
            "f10" => set_key(&mut code, Code::F10)?,
            "f11" => set_key(&mut code, Code::F11)?,
            "f12" => set_key(&mut code, Code::F12)?,
            // Special keys
            "space" => set_key(&mut code, Code::Space)?,
            "enter" | "return" => set_key(&mut code, Code::Enter)?,
            "escape" | "esc" => set_key(&mut code, Code::Escape)?,
            "tab" => set_key(&mut code, Code::Tab)?,
            "backspace" => set_key(&mut code, Code::Backspace)?,
            // Arrows (HotkeyRecorder emits Up/Down/Left/Right)
            "up" | "arrowup" => set_key(&mut code, Code::ArrowUp)?,
            "down" | "arrowdown" => set_key(&mut code, Code::ArrowDown)?,
            "left" | "arrowleft" => set_key(&mut code, Code::ArrowLeft)?,
            "right" | "arrowright" => set_key(&mut code, Code::ArrowRight)?,
            _ => return None,
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

#[cfg(test)]
mod tests {

    #[test]
    fn live_preview_drops_non_speech_markers() {
        assert_eq!(crate::engines::whisper::strip_non_speech("[BLANK_AUDIO]"), "");
        assert_eq!(crate::engines::whisper::strip_non_speech(" Hello (music) world [inaudible] "), "Hello world");
        assert_eq!(crate::engines::whisper::strip_non_speech("It's 10:30, okay?"), "It's 10:30, okay?");
    }

    use super::*;

    /// Every key name HotkeyRecorder's KEY_TO_BACKEND can emit.
    fn recorder_keys() -> Vec<(String, Code)> {
        let mut v: Vec<(String, Code)> = Vec::new();
        let letters = [
            Code::KeyA, Code::KeyB, Code::KeyC, Code::KeyD, Code::KeyE, Code::KeyF, Code::KeyG,
            Code::KeyH, Code::KeyI, Code::KeyJ, Code::KeyK, Code::KeyL, Code::KeyM, Code::KeyN,
            Code::KeyO, Code::KeyP, Code::KeyQ, Code::KeyR, Code::KeyS, Code::KeyT, Code::KeyU,
            Code::KeyV, Code::KeyW, Code::KeyX, Code::KeyY, Code::KeyZ,
        ];
        for (i, c) in letters.iter().enumerate() {
            v.push((((b'A' + i as u8) as char).to_string(), *c));
        }
        let digits = [
            Code::Digit0, Code::Digit1, Code::Digit2, Code::Digit3, Code::Digit4,
            Code::Digit5, Code::Digit6, Code::Digit7, Code::Digit8, Code::Digit9,
        ];
        for (i, c) in digits.iter().enumerate() {
            v.push((i.to_string(), *c));
        }
        let fkeys = [
            Code::F1, Code::F2, Code::F3, Code::F4, Code::F5, Code::F6,
            Code::F7, Code::F8, Code::F9, Code::F10, Code::F11, Code::F12,
        ];
        for (i, c) in fkeys.iter().enumerate() {
            v.push((format!("F{}", i + 1), *c));
        }
        for (n, c) in [
            ("Space", Code::Space), ("Enter", Code::Enter), ("Escape", Code::Escape),
            ("Tab", Code::Tab), ("Backspace", Code::Backspace), ("Up", Code::ArrowUp),
            ("Down", Code::ArrowDown), ("Left", Code::ArrowLeft), ("Right", Code::ArrowRight),
        ] {
            v.push((n.to_string(), c));
        }
        v
    }

    #[test]
    fn parses_every_string_the_recorder_can_emit() {
        let keys = recorder_keys();
        assert_eq!(keys.len(), 57, "recorder key table out of sync");
        // Modifier combos in the order the recorder emits them.
        let combos: [(&str, Modifiers); 5] = [
            ("CommandOrControl", Modifiers::SUPER),
            ("Control", Modifiers::CONTROL),
            ("Alt", Modifiers::ALT),
            ("CommandOrControl+Shift", Modifiers::SUPER | Modifiers::SHIFT),
            ("Control+Shift+Alt", Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::ALT),
        ];
        let mut ran = 0;
        for (mods_str, mods) in combos {
            for (name, code) in &keys {
                let s = format!("{}+{}", mods_str, name);
                let parsed = parse_shortcut(&s);
                assert_eq!(parsed, Some(Shortcut::new(Some(mods), *code)), "failed for {s}");
                ran += 1;
            }
        }
        assert_eq!(ran, 285);
    }

    #[test]
    fn parses_defaults_and_aliases() {
        assert_eq!(
            parse_shortcut("Option+S"),
            Some(Shortcut::new(Some(Modifiers::ALT), Code::KeyS))
        );
        assert_eq!(parse_shortcut("esc"), Some(Shortcut::new(None, Code::Escape)));
        assert_eq!(
            parse_shortcut(" cmd + shift + d "),
            Some(Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyD))
        );
    }

    #[test]
    fn rejects_empty_unknown_and_malformed() {
        for bad in ["", "   ", "+", "Shift", "Alt+Shift", "Alt+Banana", "Alt+", "A+B", "Alt+S+D", "RightOption"] {
            assert_eq!(parse_shortcut(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn validate_hotkey_cases() {
        assert!(validate_hotkey("Dictation", "RightOption", true).is_ok());
        assert!(validate_hotkey("Read aloud", "RightOption", false).is_err());
        assert!(validate_hotkey("Dictation", "", true).unwrap_err().contains("not set"));
        assert!(validate_hotkey("Dictation", "Alt+Nope", true).unwrap_err().contains("not a valid"));
        assert!(validate_hotkey("Dictation", "Alt+Up", true).is_ok());
    }
}
