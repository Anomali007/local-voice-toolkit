use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Bumped every time the overlay is shown. Delayed hides capture the value at
/// scheduling time and skip hiding if a newer session has started since.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn overlay_generation() -> u64 {
    OVERLAY_GENERATION.load(Ordering::SeqCst)
}

/// Which face the overlay window is showing; decides its size and whether it
/// takes mouse events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Dictation,
    Reading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopCenter,
    TopLeft,
    TopRight,
    BottomCenter,
    BottomLeft,
    BottomRight,
}

impl Anchor {
    /// Unknown strings fall back to the original top-center.
    pub fn parse(name: &str) -> Self {
        match name {
            "top-left" => Self::TopLeft,
            "top-right" => Self::TopRight,
            "bottom-center" => Self::BottomCenter,
            "bottom-left" => Self::BottomLeft,
            "bottom-right" => Self::BottomRight,
            _ => Self::TopCenter,
        }
    }
}

/// Window and type scale for the size presets.
pub fn size_scale(name: &str) -> f64 {
    match name {
        "compact" => 0.85,
        "large" => 1.2,
        _ => 1.0,
    }
}

pub const MIN_LINES: u8 = 2;
pub const MAX_LINES: u8 = 8;
const BASE_WIDTH: f64 = 400.0;
/// Window height at 4 transcript lines, regular size (matches tauri.conf.json).
const BASE_HEIGHT: f64 = 230.0;
/// Extra window height per additional transcript line (17px type, 1.375 leading).
const LINE_HEIGHT: f64 = 24.0;
/// The read-aloud pill, logical points. The window is exactly this size.
pub const READING_SIZE: (f64, f64) = (320.0, 64.0);
/// Gap between the overlay and the screen edge, logical px.
const MARGIN: f64 = 16.0;

/// Logical (point) size of the window for a mode, size preset and line count.
pub fn logical_size(mode: OverlayMode, size: &str, lines: u8) -> (f64, f64) {
    let scale = size_scale(size);
    let height = match mode {
        OverlayMode::Dictation => {
            BASE_HEIGHT + (lines.clamp(MIN_LINES, MAX_LINES) as f64 - 4.0) * LINE_HEIGHT
        }
        OverlayMode::Reading => return READING_SIZE,
    };
    (BASE_WIDTH * scale, height * scale)
}

/// A monitor's usable area in physical pixels and its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorArea {
    pub work: PhysRect,
    pub scale: f64,
}

impl MonitorArea {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.work.x
            && y >= self.work.y
            && x < self.work.x + self.work.width as i32
            && y < self.work.y + self.work.height as i32
    }
}

/// Physical window position -> the logical point saved in settings (physical
/// / scale of the monitor the window's top-left is on). `None` when it is on
/// no monitor's usable area.
pub fn to_saved_point(phys: (i32, i32), monitors: &[MonitorArea]) -> Option<(i32, i32)> {
    let m = monitors.iter().find(|m| m.contains(phys.0, phys.1))?;
    Some(((phys.0 as f64 / m.scale).round() as i32, (phys.1 as f64 / m.scale).round() as i32))
}

/// Saved logical point -> physical (x, y, w, h) for a pill of `logical` size.
/// The first monitor whose usable area holds the point wins (with mixed scale
/// factors logical spaces can overlap; top-left inside is the test). The pill
/// is nudged to fit inside that area. `None` when the spot is on no monitor
/// (e.g. the display was unplugged) so the caller falls back to the default.
pub fn restore_saved(
    saved: (i32, i32),
    monitors: &[MonitorArea],
    logical: (f64, f64),
) -> Option<(i32, i32, u32, u32)> {
    monitors.iter().find_map(|m| {
        let px = (saved.0 as f64 * m.scale).round() as i32;
        let py = (saved.1 as f64 * m.scale).round() as i32;
        if !m.contains(px, py) {
            return None;
        }
        let w = (logical.0 * m.scale).round() as i32;
        let h = (logical.1 * m.scale).round() as i32;
        let x = px.min(m.work.x + m.work.width as i32 - w).max(m.work.x);
        let y = py.min(m.work.y + m.work.height as i32 - h).max(m.work.y);
        Some((x, y, w.max(1) as u32, h.max(1) as u32))
    })
}

/// True while the window shows the reading pill (so moves are the user's).
static READING_ACTIVE: AtomicBool = AtomicBool::new(false);
/// When we last moved the window ourselves; the resulting Moved events are
/// not drags.
static LAST_PLACED: Mutex<Option<Instant>> = Mutex::new(None);
static MOVE_SEQ: AtomicU64 = AtomicU64::new(0);
const PLACEMENT_QUIET: Duration = Duration::from_millis(500);
const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);

fn monitor_areas(app: &AppHandle) -> Vec<MonitorArea> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let w = m.work_area();
            MonitorArea {
                work: PhysRect {
                    x: w.position.x,
                    y: w.position.y,
                    width: w.size.width,
                    height: w.size.height,
                },
                scale: m.scale_factor(),
            }
        })
        .collect()
}

/// A Moved event on the overlay window: if it is a user drag of the reading
/// pill, save the spot once the drag settles.
fn on_overlay_moved(app: &AppHandle, x: i32, y: i32) {
    if !READING_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let recently_placed = LAST_PLACED
        .lock()
        .ok()
        .and_then(|t| *t)
        .is_some_and(|t| t.elapsed() < PLACEMENT_QUIET);
    if recently_placed {
        return;
    }
    let seq = MOVE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SAVE_DEBOUNCE);
        if MOVE_SEQ.load(Ordering::SeqCst) != seq || !READING_ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        let Some((sx, sy)) = to_saved_point((x, y), &monitor_areas(&app)) else { return };
        let saved = crate::commands::settings::modify_settings(|s| {
            s.reading_overlay_x = sx;
            s.reading_overlay_y = sy;
            s.reading_overlay_custom = true;
        });
        if let Err(e) = saved {
            tracing::warn!("Could not save reading overlay position: {}", e);
        }
    });
}

/// Forget the dragged spot: the next reading uses the default again.
#[tauri::command]
pub fn reset_reading_overlay_position() -> Result<(), String> {
    crate::commands::settings::modify_settings(|s| {
        s.reading_overlay_x = 0;
        s.reading_overlay_y = 0;
        s.reading_overlay_custom = false;
    })
}

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Physical top-left position and size for a window of `logical` points on a
/// monitor whose usable area (below the menu bar, above the dock) is `work`
/// and whose scale factor is `scale`. `offset` is a logical nudge (right/down
/// positive). The result is kept inside `work`.
pub fn place(
    work: PhysRect,
    scale: f64,
    logical: (f64, f64),
    anchor: Anchor,
    offset: (i32, i32),
) -> (i32, i32, u32, u32) {
    let w = (logical.0 * scale).round() as i32;
    let h = (logical.1 * scale).round() as i32;
    let margin = (MARGIN * scale).round() as i32;
    let (work_w, work_h) = (work.width as i32, work.height as i32);

    let x = match anchor {
        Anchor::TopLeft | Anchor::BottomLeft => work.x + margin,
        Anchor::TopCenter | Anchor::BottomCenter => work.x + (work_w - w) / 2,
        Anchor::TopRight | Anchor::BottomRight => work.x + work_w - w - margin,
    };
    let y = match anchor {
        Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight => work.y + margin,
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => {
            work.y + work_h - h - margin
        }
    };
    let x = x + (offset.0 as f64 * scale).round() as i32;
    let y = y + (offset.1 as f64 * scale).round() as i32;

    // Never push the window off the usable area (a window bigger than the
    // area is pinned to its top-left).
    let x = x.min(work.x + work_w - w).max(work.x);
    let y = y.min(work.y + work_h - h).max(work.y);
    (x, y, w.max(1) as u32, h.max(1) as u32)
}

/// Let the overlay float over full-screen apps and appear on every Space.
/// Tauri's visibleOnAllWorkspaces sets only CanJoinAllSpaces, so a dictation
/// started inside a full-screen app (or on another desktop) showed nothing.
#[cfg(target_os = "macos")]
fn float_over_everything(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // NSWindowCollectionBehavior: CanJoinAllSpaces | Stationary |
    // IgnoresCycle | FullScreenAuxiliary
    const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
    // NSStatusWindowLevel: above normal and full-screen windows
    const STATUS_LEVEL: isize = 25;

    match window.ns_window() {
        Ok(ptr) if !ptr.is_null() => {
            let ns_window = ptr as *mut AnyObject;
            // SAFETY: a live NSWindow owned by Tauri; called on the main thread.
            unsafe {
                let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
                let _: () = msg_send![ns_window, setLevel: STATUS_LEVEL];
            }
        }
        _ => tracing::warn!("Overlay has no NSWindow to configure"),
    }
}

/// Turn the overlay into a non-activating panel. macOS keeps ordinary
/// windows of other apps off a full-screen app's Space even with
/// FullScreenAuxiliary set; a non-activating NSPanel (what floating
/// dictation and launcher apps use) can appear there without taking focus
/// from the app being dictated into.
#[cfg(target_os = "macos")]
fn make_panel(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    // NSWindowStyleMaskNonactivatingPanel
    const NONACTIVATING_PANEL: usize = 1 << 7;

    let Ok(ptr) = window.ns_window() else { return };
    let Some(panel_class) = AnyClass::get(c"NSPanel") else { return };
    if ptr.is_null() {
        return;
    }
    let ns_window = ptr as *mut AnyObject;
    // SAFETY: NSPanel is an NSWindow subclass with no extra instance
    // variables, so re-classing a live NSWindow to it is sound (the same
    // approach tauri-nspanel uses). Main thread, during setup.
    unsafe {
        objc2::ffi::object_setClass(ns_window, panel_class);
        let mask: usize = msg_send![ns_window, styleMask];
        let _: () = msg_send![ns_window, setStyleMask: mask | NONACTIVATING_PANEL];
        let _: () = msg_send![ns_window, setFloatingPanel: true];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        let _: () = msg_send![ns_window, setBecomesKeyOnlyIfNeeded: true];
        // The window server reads "nonactivating" only when a window is
        // created, so the mask above does not stop a click from activating
        // Blah³ (which brings the main window forward). This private setter
        // updates it; guarded so a future macOS without it just skips it.
        let prevents = objc2::sel!(_setPreventsActivation:);
        let responds: bool = msg_send![ns_window, respondsToSelector: prevents];
        if responds {
            let _: () = msg_send![ns_window, _setPreventsActivation: true];
        } else {
            tracing::warn!("Overlay cannot opt out of activation on this macOS");
        }
    }
    tracing::info!("Dictation overlay is a non-activating panel");
}

/// Configure the overlay window once at startup.
pub fn setup_overlay(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        make_panel(&window);
        float_over_everything(&window);
    }
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        let handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Moved(pos) = event {
                on_overlay_moved(&handle, pos.x, pos.y);
            }
        });
    }
}

/// Size, position and mouse behaviour for `mode`, from the saved settings.
fn configure_window(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    mode: OverlayMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = crate::commands::settings::get_settings().unwrap_or_default();

    // The monitor with the mouse is where the user is looking.
    let cursor_monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten());
    let monitor = match cursor_monitor {
        Some(m) => Some(m),
        None => window.current_monitor()?.or(window.primary_monitor()?),
    };
    // Moves made below are ours, not the user's drags.
    READING_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut t) = LAST_PLACED.lock() {
        *t = Some(Instant::now());
    }
    let logical = logical_size(mode, &settings.overlay_size, settings.overlay_max_lines);
    let saved = (mode == OverlayMode::Reading && settings.reading_overlay_custom)
        .then(|| {
            restore_saved(
                (settings.reading_overlay_x, settings.reading_overlay_y),
                &monitor_areas(app),
                logical,
            )
        })
        .flatten();
    let placement = saved.or_else(|| {
        let monitor = monitor?;
        let work = monitor.work_area();
        let work = PhysRect {
            x: work.position.x,
            y: work.position.y,
            width: work.size.width,
            height: work.size.height,
        };
        Some(match mode {
            OverlayMode::Reading => {
                place(work, monitor.scale_factor(), logical, Anchor::BottomCenter, (0, 0))
            }
            OverlayMode::Dictation => place(
                work,
                monitor.scale_factor(),
                logical,
                Anchor::parse(&settings.overlay_position),
                (settings.overlay_offset_x, settings.overlay_offset_y),
            ),
        })
    });
    if let Some((x, y, w, h)) = placement {
        // Physical units on both calls: the window may still sit on a monitor
        // with a different scale factor than the one it is moving to.
        window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width: w, height: h }))?;
        window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }))?;
    }
    if let Ok(mut t) = LAST_PLACED.lock() {
        *t = Some(Instant::now());
    }
    READING_ACTIVE.store(mode == OverlayMode::Reading, Ordering::SeqCst);

    // Reading shows buttons, so it must take clicks; dictation follows the setting.
    let click_through = mode == OverlayMode::Dictation && settings.overlay_click_through;
    window.set_ignore_cursor_events(click_through)?;
    Ok(())
}

fn show_in_mode(app: &AppHandle, mode: OverlayMode) -> Result<u64, Box<dyn std::error::Error>> {
    let generation = OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        #[cfg(target_os = "macos")]
        float_over_everything(&window);
        configure_window(app, &window, mode)?;
        window.show()?;
        tracing::debug!("Overlay shown ({:?})", mode);
    } else {
        tracing::warn!("Dictation overlay window not found");
    }
    Ok(generation)
}

/// Show the dictation overlay at the configured position and size.
pub fn show_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    show_in_mode(app, OverlayMode::Dictation).map(|_| ())
}

/// Show the read-aloud overlay. Returns the generation to hand back to
/// `overlay_generation()` when deciding whether it is still ours to hide, or
/// `None` when a dictation owns the overlay and nothing was shown.
pub fn show_reading_overlay(app: &AppHandle) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    use std::sync::Arc;
    if let Some(state) = app.try_state::<Arc<crate::hotkeys::HotkeyState>>() {
        if state.is_recording.load(Ordering::SeqCst) {
            return Ok(None);
        }
    }
    show_in_mode(app, OverlayMode::Reading).map(Some)
}

/// Show the overlay for about 3 s with sample text so the Overlay settings
/// can be judged without dictating.
#[tauri::command]
pub async fn preview_overlay(app: AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    use std::sync::Arc;
    let recording = app
        .try_state::<Arc<crate::hotkeys::HotkeyState>>()
        .is_some_and(|s| s.is_recording.load(Ordering::SeqCst));
    if recording || crate::commands::tts::is_speaking() {
        return Ok(());
    }
    let generation = show_in_mode(&app, OverlayMode::Dictation).map_err(|e| e.to_string())?;
    let _ = app.emit("stt-recording-started", serde_json::json!({ "target_app": null }));
    let _ = app.emit(
        "stt-partial-result",
        "This is how your dictation overlay looks. Words appear here as you speak, and the oldest lines scroll away once the limit you picked is reached, so the newest words stay in view.",
    );
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    if overlay_generation() == generation {
        let _ = hide_overlay(&app);
    }
    Ok(())
}

/// Hide the dictation overlay window
pub fn hide_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    READING_ACTIVE.store(false, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        window.hide()?;
        tracing::debug!("Dictation overlay hidden");
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, width: u32, height: u32) -> PhysRect {
        PhysRect { x, y, width, height }
    }

    #[test]
    fn top_center_on_a_1x_display() {
        // 1440x900 with a 25px menu bar: work area starts at y=25.
        let (x, y, w, h) = place(rect(0, 25, 1440, 875), 1.0, (400.0, 230.0), Anchor::TopCenter, (0, 0));
        assert_eq!((x, y, w, h), (520, 41, 400, 230));
    }

    #[test]
    fn retina_doubles_size_margin_and_offset() {
        // Same screen at scale 2.0: everything is in physical pixels.
        let work = rect(0, 50, 2880, 1750);
        let (x, y, w, h) = place(work, 2.0, (400.0, 230.0), Anchor::TopCenter, (0, 0));
        assert_eq!((w, h), (800, 460));
        assert_eq!((x, y), (1040, 50 + 32));
        // A 10 pt nudge right/down is 20 physical px.
        let (x2, y2, ..) = place(work, 2.0, (400.0, 230.0), Anchor::TopCenter, (10, 10));
        assert_eq!((x2 - x, y2 - y), (20, 20));
    }

    #[test]
    fn corners_and_bottom_respect_the_work_area() {
        let work = rect(0, 25, 1440, 800); // dock took the bottom 75px
        let logical = (400.0, 230.0);
        assert_eq!(place(work, 1.0, logical, Anchor::TopLeft, (0, 0)).0, 16);
        assert_eq!(place(work, 1.0, logical, Anchor::TopRight, (0, 0)).0, 1440 - 400 - 16);
        let (x, y, ..) = place(work, 1.0, logical, Anchor::BottomCenter, (0, 0));
        assert_eq!((x, y), (520, 25 + 800 - 230 - 16));
        let (x, y, ..) = place(work, 1.0, logical, Anchor::BottomRight, (0, 0));
        assert_eq!((x, y), (1440 - 400 - 16, 25 + 800 - 230 - 16));
        let (x, ..) = place(work, 1.0, logical, Anchor::BottomLeft, (0, 0));
        assert_eq!(x, 16);
    }

    #[test]
    fn secondary_monitor_origins_are_honoured() {
        // Monitor to the right, retina.
        let right = rect(2880, 0, 2560, 1440);
        let (x, y, w, _) = place(right, 2.0, (400.0, 230.0), Anchor::TopRight, (0, 0));
        assert_eq!(w, 800);
        assert_eq!((x, y), (2880 + 2560 - 800 - 32, 32));
        // Monitor to the left and above the primary (negative origin).
        let left = rect(-1920, -200, 1920, 1080);
        let (x, y, ..) = place(left, 1.0, (400.0, 230.0), Anchor::TopCenter, (0, 0));
        assert_eq!((x, y), (-1920 + 760, -200 + 16));
        let (x, y, ..) = place(left, 1.0, (400.0, 230.0), Anchor::BottomLeft, (0, 0));
        assert_eq!((x, y), (-1920 + 16, -200 + 1080 - 230 - 16));
    }

    #[test]
    fn nudge_cannot_push_the_window_off_the_screen() {
        let work = rect(0, 25, 1440, 800);
        let (x, y, ..) = place(work, 1.0, (400.0, 230.0), Anchor::TopLeft, (-5000, -5000));
        assert_eq!((x, y), (0, 25));
        let (x, y, ..) = place(work, 1.0, (400.0, 230.0), Anchor::BottomRight, (5000, 5000));
        assert_eq!((x, y), (1040, 25 + 800 - 230));
    }

    #[test]
    fn sizes_scale_and_default_matches_the_configured_window() {
        assert_eq!(logical_size(OverlayMode::Dictation, "regular", 4), (400.0, 230.0));
        let (cw, ch) = logical_size(OverlayMode::Dictation, "compact", 4);
        assert!((cw - 340.0).abs() < 1e-9 && (ch - 195.5).abs() < 1e-9);
        assert_eq!(logical_size(OverlayMode::Dictation, "large", 4).0, 480.0);
        assert_eq!(logical_size(OverlayMode::Dictation, "bogus", 4), (400.0, 230.0));
    }

    #[test]
    fn more_lines_make_a_taller_window_within_bounds() {
        let h = |lines| logical_size(OverlayMode::Dictation, "regular", lines).1;
        assert_eq!(h(8) - h(4), 96.0);
        assert_eq!(h(1), h(2)); // clamped to 2..=8
        assert_eq!(h(30), h(8));
        assert!(logical_size(OverlayMode::Reading, "regular", 8).1 < h(4));
    }

    fn mon(x: i32, y: i32, w: u32, h: u32, scale: f64) -> MonitorArea {
        MonitorArea { work: rect(x, y, w, h), scale }
    }

    #[test]
    fn reading_pill_is_small_and_ignores_the_size_preset() {
        let (w, h) = logical_size(OverlayMode::Reading, "large", 8);
        assert_eq!((w, h), READING_SIZE);
        assert!((280.0..=320.0).contains(&w) && (56.0..=64.0).contains(&h));
    }

    #[test]
    fn saved_spot_on_screen_is_restored() {
        let monitors = [mon(0, 25, 1440, 875, 1.0)];
        assert_eq!(restore_saved((300, 400), &monitors, READING_SIZE), Some((300, 400, 320, 64)));
    }

    #[test]
    fn saved_spot_scales_on_retina_and_round_trips() {
        let monitors = [mon(0, 50, 2880, 1750, 2.0)];
        // Dragged to physical (1000, 900) -> saved as points -> restored.
        let saved = to_saved_point((1000, 900), &monitors).unwrap();
        assert_eq!(saved, (500, 450));
        assert_eq!(restore_saved(saved, &monitors, READING_SIZE), Some((1000, 900, 640, 128)));
    }

    #[test]
    fn saved_spot_on_an_unplugged_monitor_falls_back() {
        // Saved while a second display sat to the right; now only one remains.
        let monitors = [mon(0, 25, 1440, 875, 1.0)];
        assert_eq!(restore_saved((2000, 300), &monitors, READING_SIZE), None);
        assert_eq!(restore_saved((-500, 300), &monitors, READING_SIZE), None);
        assert_eq!(restore_saved((300, 5000), &monitors, READING_SIZE), None);
    }

    #[test]
    fn saved_spot_on_a_second_monitor_is_found() {
        let monitors = [mon(0, 25, 1440, 875, 1.0), mon(1440, 0, 1920, 1080, 1.0)];
        assert_eq!(restore_saved((1600, 100), &monitors, READING_SIZE), Some((1600, 100, 320, 64)));
    }

    #[test]
    fn restored_pill_is_pulled_inside_the_work_area() {
        // Top-left is on screen but the pill would hang off the right/bottom edge.
        let monitors = [mon(0, 25, 1440, 875, 1.0)];
        assert_eq!(restore_saved((1430, 895), &monitors, READING_SIZE), Some((1120, 836, 320, 64)));
    }

    #[test]
    fn drag_off_every_monitor_is_not_saved() {
        let monitors = [mon(0, 25, 1440, 875, 1.0)];
        assert_eq!(to_saved_point((-50, -50), &monitors), None);
    }

    #[test]
    fn unknown_anchor_falls_back_to_top_center() {
        assert_eq!(Anchor::parse("nonsense"), Anchor::TopCenter);
        assert_eq!(Anchor::parse("bottom-right"), Anchor::BottomRight);
    }
}
