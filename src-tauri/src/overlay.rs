use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

/// Bumped every time the overlay is shown. Delayed hides capture the value at
/// scheduling time and skip hiding if a newer session has started since.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn overlay_generation() -> u64 {
    OVERLAY_GENERATION.load(Ordering::SeqCst)
}

/// Compute the top-center position for a window of `window_width` on a
/// monitor at `(monitor_x, monitor_y)` with width `monitor_width` (all physical px).
fn top_center(monitor_x: i32, monitor_y: i32, monitor_width: u32, window_width: u32) -> (i32, i32) {
    let x = monitor_x + (monitor_width as i32 - window_width as i32) / 2;
    (x, monitor_y + 50) // 50px from the top of that monitor
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
}

/// Show the dictation overlay window positioned at top-center of screen
pub fn show_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        #[cfg(target_os = "macos")]
        float_over_everything(&window);

        // Get the primary monitor to center horizontally at top
        // The monitor with the mouse is where the user is looking.
        let cursor_monitor = app
            .cursor_position()
            .ok()
            .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten());
        let monitor = match cursor_monitor {
            Some(m) => Some(m),
            None => window.current_monitor()?.or(window.primary_monitor()?),
        };
        if let Some(monitor) = monitor {
            let monitor_size = monitor.size();
            let window_size = window.outer_size()?;

            // Position at top-center with some padding from the top
            let origin = monitor.position();
            let (x, y) = top_center(origin.x, origin.y, monitor_size.width, window_size.width);

            window.set_position(tauri::Position::Physical(tauri::PhysicalPosition { x, y }))?;
        }

        window.show()?;
        tracing::debug!("Dictation overlay shown");
    } else {
        tracing::warn!("Dictation overlay window not found");
    }

    Ok(())
}

/// Hide the dictation overlay window
pub fn hide_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        window.hide()?;
        tracing::debug!("Dictation overlay hidden");
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_on_primary_monitor() {
        assert_eq!(top_center(0, 0, 3000, 400), (1300, 50));
    }

    #[test]
    fn respects_secondary_monitor_offsets() {
        assert_eq!(top_center(3000, 0, 2000, 400), (3800, 50));
        assert_eq!(top_center(-1920, -200, 1920, 400), (-1160, -150));
    }
}
