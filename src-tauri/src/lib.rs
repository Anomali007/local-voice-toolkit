mod accessibility;
mod audio;
mod commands;
mod engines;
mod hotkeys;
mod models;

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

            // Show main window on startup (for development)
            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::stt::start_recording,
            commands::stt::stop_recording,
            commands::stt::transcribe_audio,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
