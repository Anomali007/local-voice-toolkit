use tauri::{AppHandle, Manager};

/// Show the dictation overlay window positioned at top-center of screen
pub fn show_overlay(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("dictation-overlay") {
        // Get the primary monitor to center horizontally at top
        if let Some(monitor) = window.current_monitor()? {
            let monitor_size = monitor.size();
            let window_size = window.outer_size()?;

            // Position at top-center with some padding from the top
            let x = (monitor_size.width as i32 - window_size.width as i32) / 2;
            let y = 50; // 50px from top

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

