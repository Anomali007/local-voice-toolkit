//! Menu bar icon and its menu.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::settings::{get_settings, update_settings, AppSettings};
use crate::engines::phonon;

const TRAY_ID: &str = "main";
const SPEEDS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon@2x.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Blah³")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle(app, event.id.as_ref()))
        .build(app)?;
    Ok(())
}

/// Rebuild the menu so checkmarks and hotkey labels match the settings.
pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match build_menu(app) {
            Ok(menu) => {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    tracing::warn!("Failed to update tray menu: {}", e);
                }
            }
            Err(e) => tracing::warn!("Failed to build tray menu: {}", e),
        }
    }
}

/// "RightOption" -> "Right ⌥", "Option+S" -> "⌥S"
pub fn hotkey_label(hotkey: &str) -> String {
    if hotkey.eq_ignore_ascii_case(crate::right_option::RIGHT_OPTION_HOTKEY) {
        return "Right ⌥".into();
    }
    hotkey
        .split('+')
        .map(|part| match part.trim().to_lowercase().as_str() {
            "command" | "commandorcontrol" | "cmd" | "super" => "⌘".to_string(),
            "control" | "ctrl" => "⌃".to_string(),
            "option" | "alt" => "⌥".to_string(),
            "shift" => "⇧".to_string(),
            other => other.to_uppercase(),
        })
        .collect()
}

fn speed_id(speed: f32) -> String {
    format!("speed:{speed}")
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let s = get_settings().unwrap_or_default();
    let phonon_ready = dirs::data_dir()
        .map(|d| d.join("com.blahcubed.app/models/tts").join(phonon::MODEL_ID))
        .is_some_and(|dir| phonon::missing_files(&dir).is_empty());
    let engine_is_phonon = s.tts_engine.eq_ignore_ascii_case("phonon");

    let dictate = MenuItem::with_id(
        app,
        "info:dictate",
        format!("Dictate: {}", hotkey_label(&s.stt_hotkey)),
        false,
        None::<&str>,
    )?;
    let read = MenuItem::with_id(
        app,
        "info:read",
        format!("Read aloud: {}", hotkey_label(&s.tts_hotkey)),
        false,
        None::<&str>,
    )?;

    let read_selection =
        MenuItem::with_id(app, "read_selection", "Read Selection Aloud", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop_speaking", "Stop Speaking", true, None::<&str>)?;

    let kokoro = CheckMenuItem::with_id(app, "engine:kokoro", "Kokoro", true, !engine_is_phonon, None::<&str>)?;
    let phonon_label = if phonon_ready { "Phonon" } else { "Phonon (download in Models)" };
    let phonon_item =
        CheckMenuItem::with_id(app, "engine:phonon", phonon_label, phonon_ready, engine_is_phonon, None::<&str>)?;
    let voice = Submenu::with_items(app, "Voice", true, &[&kokoro, &phonon_item])?;

    let speed_items = SPEEDS
        .iter()
        .map(|&v| {
            CheckMenuItem::with_id(
                app,
                speed_id(v),
                format!("{v}x"),
                true,
                (s.tts_speed - v).abs() < 0.01,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let speed_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        speed_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<Wry>).collect();
    let speed = Submenu::with_items(app, "Speed", true, &speed_refs)?;

    let show = MenuItem::with_id(app, "show", "Open Blah³", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit Blah³", true, Some("CmdOrCtrl+Q"))?;

    Menu::with_items(
        app,
        &[
            &dictate,
            &read,
            &PredefinedMenuItem::separator(app)?,
            &read_selection,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &voice,
            &speed,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Apply a settings change made from the menu, and tell the UI.
fn change_settings(app: &AppHandle, edit: impl FnOnce(&mut AppSettings)) {
    let mut s = get_settings().unwrap_or_default();
    edit(&mut s);
    // update_settings saves, refreshes hotkeys if needed, and rebuilds this menu.
    match update_settings(app.clone(), s) {
        Ok(()) => {
            let _ = app.emit("settings-changed", ());
        }
        Err(e) => tracing::warn!("Menu settings change failed: {}", e),
    }
}

fn handle(app: &AppHandle, id: &str) {
    match id {
        "read_selection" => crate::hotkeys::toggle_read_selection(app),
        "stop_speaking" => {
            tauri::async_runtime::spawn(crate::commands::tts::stop_speaking());
        }
        "engine:kokoro" => change_settings(app, |s| {
            s.tts_engine = "kokoro".into();
            if s.tts_voice == "default" {
                s.tts_voice = "af_heart".into();
            }
        }),
        "engine:phonon" => change_settings(app, |s| {
            s.tts_engine = "phonon".into();
            s.tts_voice = "default".into();
        }),
        "show" => show_main(app),
        "settings" => {
            show_main(app);
            let _ = app.emit("navigate", "settings");
        }
        "quit" => app.exit(0),
        other => {
            if let Some(speed) = other.strip_prefix("speed:").and_then(|v| v.parse::<f32>().ok()) {
                change_settings(app, |s| s.tts_speed = speed);
            } else {
                // Info rows are disabled; anything else is a no-op.
                refresh(app);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_labels() {
        assert_eq!(hotkey_label("RightOption"), "Right ⌥");
        assert_eq!(hotkey_label("Option+S"), "⌥S");
        assert_eq!(hotkey_label("CommandOrControl+Shift+D"), "⌘⇧D");
        assert_eq!(hotkey_label("Control+Alt+Space"), "⌃⌥SPACE");
    }

    #[test]
    fn speed_ids_round_trip() {
        for v in SPEEDS {
            let id = speed_id(v);
            let parsed: f32 = id.strip_prefix("speed:").unwrap().parse().unwrap();
            assert!((parsed - v).abs() < f32::EPSILON, "{id}");
        }
    }
}
