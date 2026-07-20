mod accessibility;
mod audio;
mod commands;
mod engines;
mod hotkeys;
mod models;
mod overlay;
mod right_option;

use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn run() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "blah3=debug,info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting Blah³...");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None, // No extra args
        ))
        .manage(Arc::new(hotkeys::HotkeyState::default()))
        .setup(|app| {
            // Create tray menu
            let show_i = MenuItem::with_id(app, "show", "Show Blah³", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            // Build tray icon
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Blah³ - Voice Toolkit")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Register global hotkeys
            if let Err(e) = hotkeys::register_hotkeys(app.handle()) {
                tracing::error!("Failed to register hotkeys: {}", e);
            } else {
                tracing::info!("Global hotkeys registered successfully");
            }

            // Preload the configured Whisper model in the background so the
            // first dictation doesn't pay the multi-second model-load cost.
            std::thread::spawn(|| {
                let settings = commands::settings::get_settings().unwrap_or_default();
                if let Some(dir) = dirs::data_dir() {
                    let model_path = dir
                        .join("com.blahcubed.app")
                        .join("models")
                        .join("stt")
                        .join(&settings.stt_model);
                    if model_path.exists() {
                        match engines::whisper::get_or_load_cached(&model_path.to_string_lossy()) {
                            Ok(_) => tracing::info!("Whisper model preloaded"),
                            Err(e) => tracing::warn!("Whisper model preload failed: {}", e),
                        }
                    } else {
                        tracing::info!(
                            "STT model {} not downloaded yet - skipping preload",
                            settings.stt_model
                        );
                    }
                }
            });

            // Show main window on startup (for development)
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Blah³ is a menu-bar app: closing the main window hides it so the
            // tray "Show Blah³" item keeps working (destroying the window
            // would leave the tray pointing at nothing until relaunch).
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::stt::start_recording,
            commands::stt::stop_recording,
            commands::stt::transcribe_audio,
            commands::stt::is_silence_triggered,
            commands::stt::is_recording,
            commands::tts::speak_text,
            commands::tts::stop_speaking,
            commands::tts::get_voices,
            commands::models::list_models,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::get_model_status,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_hardware_info,
            commands::permissions::check_permissions,
            commands::permissions::open_system_settings,
            commands::permissions::request_microphone_access,
            commands::permissions::request_accessibility_access,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
