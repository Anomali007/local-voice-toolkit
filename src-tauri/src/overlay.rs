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

/// Show the dictation overlay window positioned at top-center of screen
pub fn show_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        // Get the primary monitor to center horizontally at top
        if let Some(monitor) = window.current_monitor()? {
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
