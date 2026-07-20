//! Right-Option bare-modifier dictation trigger (Fluid Voice style).
//!
//! `tauri-plugin-global-shortcut` accelerators cannot express a lone modifier
//! key, so this module runs a listen-only CGEventTap on a dedicated thread and
//! watches `flagsChanged` events for the right Option key (keycode 61).
//!
//! Semantics (mirrors the accelerator path in `hotkeys.rs`):
//! - Tap right-Option: start recording; tap again: stop and transcribe.
//! - Hold right-Option >500ms push-to-talk style: release stops.
//! - Escape cancels while recording (handled by `hotkeys.rs`).
//!
//! Combo suppression: if any other key is pressed while right-Option is down,
//! the press is treated as a keyboard combo (e.g. Option+letter for special
//! characters), not a dictation gesture. A recording started by that press is
//! cancelled, and a tap-to-stop is ignored so an active recording continues.
//! Left Option is ignored entirely (distinguished via the device-specific
//! right-Alt flag bit).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager};

use crate::hotkeys::{self, HotkeyState};

/// Sentinel value stored in `AppSettings::stt_hotkey` to select this trigger.
pub const RIGHT_OPTION_HOTKEY: &str = "RightOption";

/// kVK_RightOption
const KEYCODE_RIGHT_OPTION: i64 = 61;
/// NX_DEVICERALTKEYMASK - device-specific "right Alt is down" flag bit.
const DEVICE_RIGHT_ALT_MASK: u64 = 0x40;

/// Whether the right-Option trigger is currently active (settings-controlled).
static ENABLED: AtomicBool = AtomicBool::new(false);
/// Whether the event-tap thread has been spawned (spawned once, lives forever).
static MONITOR_STARTED: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();

// Per-press session state, only touched on the main thread (dispatch order is
// preserved by run_on_main_thread, so these act like a tiny state machine).
static STARTED_BY_PRESS: AtomicBool = AtomicBool::new(false);
static COMBO_SEEN: AtomicBool = AtomicBool::new(false);

/// Enable/disable the right-Option trigger. Called from hotkey (re)registration.
/// The event-tap thread is started lazily on first enable and then kept alive;
/// the `ENABLED` flag gates all handling so disabling is instant.
pub fn set_enabled(app: &AppHandle, enabled: bool) {
    ENABLED.store(enabled, Ordering::SeqCst);
    if enabled {
        let _ = APP.set(app.clone());
        if !MONITOR_STARTED.swap(true, Ordering::SeqCst) {
            if let Err(e) = std::thread::Builder::new()
                .name("right-option-monitor".into())
                .spawn(run_event_tap)
            {
                tracing::error!("Failed to spawn right-Option monitor thread: {}", e);
                MONITOR_STARTED.store(false, Ordering::SeqCst);
                return;
            }
        }
        tracing::info!("Right-Option dictation trigger enabled");
    } else {
        tracing::info!("Right-Option dictation trigger disabled");
    }
}

/// Run `f` on the main thread (same thread the accelerator callbacks use).
fn dispatch(f: impl FnOnce(&AppHandle) + Send + 'static) {
    if let Some(app) = APP.get() {
        let app_clone = app.clone();
        if let Err(e) = app.run_on_main_thread(move || f(&app_clone)) {
            tracing::warn!("Failed to dispatch right-Option event to main thread: {}", e);
        }
    }
}

/// Right-Option went down.
fn on_pressed() {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    dispatch(|app| {
        COMBO_SEEN.store(false, Ordering::SeqCst);
        let state = app.state::<Arc<HotkeyState>>();
        if state
            .is_recording
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            tracing::info!("Right-Option pressed - starting recording");
            STARTED_BY_PRESS.store(true, Ordering::SeqCst);
            hotkeys::start_dictation(app);
        } else {
            // Already recording: this press is a toggle-off candidate.
            // Act on the release so Option+letter combos don't stop recording.
            STARTED_BY_PRESS.store(false, Ordering::SeqCst);
        }
    });
}

/// Another key went down while right-Option was held: it's a combo, not a
/// dictation gesture.
fn on_combo() {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    dispatch(|app| {
        if COMBO_SEEN.swap(true, Ordering::SeqCst) {
            return; // Already handled for this press
        }
        if STARTED_BY_PRESS.load(Ordering::SeqCst) {
            tracing::info!("Right-Option combo detected - cancelling dictation");
            hotkeys::cancel_dictation(app);
        }
        // If recording was already active before this press, leave it running.
    });
}

/// Right-Option came back up.
fn on_released() {
    if !ENABLED.load(Ordering::SeqCst) {
        return;
    }
    dispatch(|app| {
        if COMBO_SEEN.load(Ordering::SeqCst) {
            return; // Combo press: nothing to do (cancel already handled)
        }
        let state = app.state::<Arc<HotkeyState>>();
        if !state.is_recording.load(Ordering::SeqCst) {
            return;
        }
        if STARTED_BY_PRESS.load(Ordering::SeqCst) {
            let held_ms = state
                .started_at
                .lock()
                .ok()
                .and_then(|guard| guard.map(|t| t.elapsed().as_millis()))
                .unwrap_or(0);
            if held_ms >= hotkeys::HOLD_THRESHOLD_MS {
                // Push-to-talk: held while speaking, release stops
                tracing::info!("Right-Option released after {}ms hold - stopping", held_ms);
                hotkeys::stop_and_transcribe(app);
            }
            // Quick tap: keep recording (toggle mode)
        } else {
            // Clean tap while already recording: toggle off
            tracing::info!("Right-Option tapped while recording - stopping");
            hotkeys::stop_and_transcribe(app);
        }
    });
}

/// Dedicated thread: create a listen-only event tap and pump its run loop.
/// If the system disables the tap (timeout/user input), it is recreated.
fn run_event_tap() {
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
        EventField,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    loop {
        // The tap silently cannot start without Accessibility permission:
        // surface the system prompt once (not on every retry).
        if !crate::commands::permissions::accessibility_trusted() {
            crate::commands::permissions::prompt_accessibility_once();
        }

        let right_down = Cell::new(false);
        let restart = Rc::new(Cell::new(false));
        let restart_cb = Rc::clone(&restart);

        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![
                CGEventType::FlagsChanged,
                CGEventType::KeyDown,
                CGEventType::TapDisabledByTimeout,
                CGEventType::TapDisabledByUserInput,
            ],
            move |_proxy, etype, event| {
                match etype {
                    CGEventType::FlagsChanged => {
                        let keycode =
                            event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        if keycode == KEYCODE_RIGHT_OPTION {
                            let down =
                                event.get_flags().bits() & DEVICE_RIGHT_ALT_MASK != 0;
                            if down && !right_down.get() {
                                right_down.set(true);
                                on_pressed();
                            } else if !down && right_down.get() {
                                right_down.set(false);
                                on_released();
                            }
                        }
                    }
                    CGEventType::KeyDown => {
                        if right_down.get() {
                            on_combo();
                        }
                    }
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                        tracing::warn!("Right-Option event tap disabled by system - recreating");
                        restart_cb.set(true);
                        CFRunLoop::get_current().stop();
                    }
                    _ => {}
                }
                None // Listen-only: never modify events
            },
        );

        match tap {
            Ok(tap) => {
                let source = match tap.mach_port.create_runloop_source(0) {
                    Ok(s) => s,
                    Err(_) => {
                        tracing::error!("Failed to create run loop source for right-Option tap");
                        return;
                    }
                };
                let run_loop = CFRunLoop::get_current();
                unsafe {
                    run_loop.add_source(&source, kCFRunLoopCommonModes);
                }
                tap.enable();
                tracing::info!("Right-Option event tap running");
                CFRunLoop::run_current();
                // run_current returned: either the tap was disabled (restart)
                // or the source went away. Fall through to the restart check.
            }
            Err(()) => {
                tracing::error!(
                    "Failed to create right-Option event tap (is Accessibility permission granted?) - retrying in 30s"
                );
                std::thread::sleep(std::time::Duration::from_secs(30));
                continue;
            }
        }

        if restart.get() {
            continue;
        }
        tracing::warn!("Right-Option event tap run loop exited");
        break;
    }
}
