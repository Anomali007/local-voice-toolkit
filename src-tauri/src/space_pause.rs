//! Hold Space to pause (FluidVoice's "Hold Space to Mute").
//!
//! While a dictation is recording, holding Space for more than
//! `HOLD_MS` pauses capture; releasing resumes. A shorter press is a normal
//! space and is typed into the target app.
//!
//! To tell a tap from a hold the key-down must be held back, so this needs a
//! NON-listen-only event tap that can swallow Space. That tap is separate from
//! the right-Option one on purpose: the right-Option tap is listen-only, always
//! on, and cannot interfere with typing; this one sits in the keyboard path and
//! so exists only while recording (created lazily, then enabled/disabled with
//! `CGEventTapEnable`; and its callback ignores everything when no recording is
//! in flight, as a second guard).
//!
//! Tap path: key-down is swallowed; if the key-up arrives within `HOLD_MS` a
//! fresh Space down+up is re-posted (tagged with `REPLAY_TAG` so this tap lets
//! it through), so the app sees a space a moment late. Hold path: after
//! `HOLD_MS` a timer pauses capture; key-up resumes.
//!
//! Stuck-pause protection: stop/cancel/abort call `deactivate()`, which
//! resumes and disables the tap; `AudioCapture::start/stop` also clear the
//! pause flag; and the callback bails out (and un-pauses) whenever
//! `is_recording` is false. Space with Cmd/Ctrl/Option held (Spotlight, input
//! source switching, right-Option combos) is never touched.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::hotkeys::HotkeyState;

pub const HOLD_MS: u64 = 250;
const KEYCODE_SPACE: i64 = 49;
/// Value written to EVENT_SOURCE_USER_DATA on the re-posted space.
const REPLAY_TAG: i64 = 0x42_33_53_50; // "B3SP"

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Whether the tap should currently be intercepting.
static WANT: AtomicBool = AtomicBool::new(false);
static THREAD_STARTED: AtomicBool = AtomicBool::new(false);
/// Raw CFMachPortRef of the tap once the thread created it (null before).
static PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static GESTURE: Mutex<SpaceGesture> = Mutex::new(SpaceGesture::new());

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
}

/// Pure tap-vs-hold state machine (the event tap feeds it; tests drive it).
#[derive(Debug)]
pub struct SpaceGesture {
    down: bool,
    paused: bool,
    gen: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Release {
    /// Key-up with no tracked key-down: let it through untouched.
    Untracked,
    /// Short press: swallow and type a space ourselves.
    Tap,
    /// End of a hold: swallow; capture resumes.
    EndPause,
}

impl SpaceGesture {
    pub const fn new() -> Self {
        Self { down: false, paused: false, gen: 0 }
    }

    /// Key-down. `Some(gen)` for a fresh press (start the hold timer for that
    /// generation); `None` for auto-repeat of the same press.
    pub fn press(&mut self) -> Option<u64> {
        if self.down {
            return None;
        }
        self.down = true;
        self.gen += 1;
        Some(self.gen)
    }

    /// The hold timer for `gen` fired. True if this starts a pause.
    pub fn timeout(&mut self, gen: u64) -> bool {
        if self.down && self.gen == gen && !self.paused {
            self.paused = true;
            return true;
        }
        false
    }

    pub fn release(&mut self) -> Release {
        if !self.down {
            return Release::Untracked;
        }
        self.down = false;
        if std::mem::take(&mut self.paused) {
            Release::EndPause
        } else {
            Release::Tap
        }
    }

    /// Forget everything. True if a pause was in effect.
    pub fn reset(&mut self) -> bool {
        self.down = false;
        std::mem::take(&mut self.paused)
    }
}

impl Default for SpaceGesture {
    fn default() -> Self {
        Self::new()
    }
}

fn gesture() -> std::sync::MutexGuard<'static, SpaceGesture> {
    GESTURE.lock().unwrap_or_else(|e| e.into_inner())
}

fn recording() -> bool {
    APP.get()
        .and_then(|app| app.try_state::<Arc<HotkeyState>>())
        .is_some_and(|s| s.is_recording.load(Ordering::SeqCst))
}

fn emit(event: &str) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, ());
    }
}

fn set_tap_enabled(enabled: bool) {
    let port = PORT.load(Ordering::SeqCst);
    if !port.is_null() {
        // SAFETY: `port` is the live mach port of the tap, which is owned by the
        // monitor thread for the life of the process (never freed).
        unsafe { CGEventTapEnable(port, enabled) };
    }
}

/// Start intercepting Space (call once capture is live). Spawns the tap thread
/// on first use. Safe to call repeatedly.
pub fn activate(app: &AppHandle) {
    let _ = APP.set(app.clone());
    WANT.store(true, Ordering::SeqCst);
    *gesture() = SpaceGesture::new();
    if !THREAD_STARTED.swap(true, Ordering::SeqCst) {
        if let Err(e) = std::thread::Builder::new().name("space-pause-tap".into()).spawn(run_tap) {
            tracing::error!("Failed to spawn Space-pause tap thread: {}", e);
            THREAD_STARTED.store(false, Ordering::SeqCst);
        }
    }
    set_tap_enabled(true);
}

/// Stop intercepting Space and resume capture if a hold was in progress.
/// Called on every way a recording can end.
pub fn deactivate() {
    WANT.store(false, Ordering::SeqCst);
    set_tap_enabled(false);
    let was_paused = gesture().reset();
    crate::audio::capture::set_paused(false);
    if was_paused {
        emit("stt-resumed");
    }
}

fn begin_pause() {
    crate::audio::capture::set_paused(true);
    emit("stt-paused");
}

fn end_pause() {
    crate::audio::capture::set_paused(false);
    emit("stt-resumed");
}

/// Re-post Space down+up so the target app types the space we held back.
fn replay_space(flags: core_graphics::event::CGEventFlags) {
    use core_graphics::event::{CGEvent, EventField};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        tracing::warn!("Space replay: no event source");
        return;
    };
    for down in [true, false] {
        if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), KEYCODE_SPACE as u16, down) {
            event.set_flags(flags);
            event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, REPLAY_TAG);
            event.post(core_graphics::event::CGEventTapLocation::HID);
        }
    }
}

fn run_tap() {
    use core_foundation::base::TCFType;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType, EventField,
    };

    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default, // active: may swallow events
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::TapDisabledByTimeout,
            CGEventType::TapDisabledByUserInput,
        ],
        move |_proxy, etype, event| {
            match etype {
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
                    // Our own CGEventTapEnable(false) also arrives here as
                    // "disabled by user input". Only act when a recording
                    // wants the tap back; calling disable again from here
                    // re-fires this event in a tight loop (it filled the log
                    // at ~16k lines/s and pinned a core).
                    if WANT.load(Ordering::SeqCst) && recording() {
                        tracing::warn!("Space-pause tap disabled by system - re-enabling");
                        set_tap_enabled(true);
                    }
                    return None;
                }
                CGEventType::KeyDown | CGEventType::KeyUp => {}
                _ => return None,
            }
            if event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) != KEYCODE_SPACE
                || event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == REPLAY_TAG
            {
                return None; // not ours: pass through
            }
            if !WANT.load(Ordering::SeqCst) || !recording() {
                // Recording is over (or never was): never hold keys hostage.
                if gesture().reset() {
                    end_pause();
                }
                set_tap_enabled(false);
                return None;
            }
            let flags = event.get_flags();
            if flags.intersects(CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagAlternate) {
                return None; // Cmd/Ctrl/Option+Space is a shortcut, not a space
            }
            // This crate's callback cannot return "drop"; re-typing the event in
            // place as Null makes the system discard it (and returning None
            // hands back that same, now-inert, event: no extra refcounts).
            let swallow = |event: &core_graphics::event::CGEvent| {
                event.set_type(CGEventType::Null);
                None
            };
            if matches!(etype, CGEventType::KeyDown) {
                let fresh = gesture().press();
                if let Some(gen) = fresh {
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(HOLD_MS));
                        if WANT.load(Ordering::SeqCst) && recording() && gesture().timeout(gen) {
                            begin_pause();
                        }
                    });
                }
                swallow(event) // first press and auto-repeats alike
            } else {
                let outcome = gesture().release();
                match outcome {
                    Release::Untracked => None,
                    Release::Tap => {
                        replay_space(flags & CGEventFlags::CGEventFlagShift);
                        swallow(event)
                    }
                    Release::EndPause => {
                        end_pause();
                        swallow(event)
                    }
                }
            }
        },
    );

    let tap = match tap {
        Ok(t) => t,
        Err(()) => {
            tracing::warn!("Could not create the Space-pause tap (Accessibility granted?) - hold-Space-to-pause unavailable");
            THREAD_STARTED.store(false, Ordering::SeqCst);
            return;
        }
    };
    let Ok(source) = tap.mach_port.create_runloop_source(0) else {
        tracing::error!("Space-pause tap: no run loop source");
        THREAD_STARTED.store(false, Ordering::SeqCst);
        return;
    };
    let run_loop = CFRunLoop::get_current();
    unsafe { run_loop.add_source(&source, kCFRunLoopCommonModes) };
    PORT.store(tap.mach_port.as_concrete_TypeRef() as *mut std::ffi::c_void, Ordering::SeqCst);
    // Start from the state the app wants right now.
    set_tap_enabled(WANT.load(Ordering::SeqCst));
    tracing::info!("Space-pause tap ready");
    CFRunLoop::run_current();

    // Run loop ended (unexpected): detach cleanly and allow a fresh spawn.
    PORT.store(std::ptr::null_mut(), Ordering::SeqCst);
    unsafe { run_loop.remove_source(&source, kCFRunLoopCommonModes) };
    THREAD_STARTED.store(false, Ordering::SeqCst);
    drop(tap);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_tap_is_a_tap() {
        let mut g = SpaceGesture::new();
        let gen = g.press().unwrap();
        assert_eq!(g.release(), Release::Tap);
        // The timer firing late must not pause anything.
        assert!(!g.timeout(gen));
    }

    #[test]
    fn hold_pauses_then_release_resumes() {
        let mut g = SpaceGesture::new();
        let gen = g.press().unwrap();
        assert!(g.press().is_none(), "auto-repeat is not a new press");
        assert!(g.timeout(gen));
        assert!(!g.timeout(gen), "pause begins once");
        assert_eq!(g.release(), Release::EndPause);
        assert_eq!(g.release(), Release::Untracked);
    }

    #[test]
    fn stale_timer_from_an_earlier_press_is_ignored() {
        let mut g = SpaceGesture::new();
        let first = g.press().unwrap();
        assert_eq!(g.release(), Release::Tap);
        let second = g.press().unwrap();
        assert_ne!(first, second);
        assert!(!g.timeout(first));
        assert!(g.timeout(second));
    }

    #[test]
    fn reset_clears_a_pause_so_it_cannot_stick() {
        let mut g = SpaceGesture::new();
        let gen = g.press().unwrap();
        assert!(g.timeout(gen));
        assert!(g.reset());
        assert!(!g.reset());
        // A key-up arriving after the reset passes through.
        assert_eq!(g.release(), Release::Untracked);
        // And a late timer does nothing.
        assert!(!g.timeout(gen));
    }

    #[test]
    fn key_up_without_a_tracked_down_is_untouched() {
        let mut g = SpaceGesture::new();
        assert_eq!(g.release(), Release::Untracked);
    }
}
