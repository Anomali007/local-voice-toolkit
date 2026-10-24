mod accessibility;
mod audio;
mod commands;
mod dictionary;
mod engines;
mod history;
mod hotkeys;
mod models;
mod overlay;
mod right_option;
mod sounds;
mod space_pause;
mod tray;

use std::sync::Arc;

use tauri::{
    Manager,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Hard ceiling for the log file in one run: a runaway log loop once wrote
/// 430 MB in half an hour. Past this, lines are dropped until next launch.
const LOG_CAP_BYTES: u64 = 20_000_000;

struct CappedLog {
    file: std::fs::File,
    written: u64,
    limit: u64,
}

impl std::io::Write for CappedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.written >= self.limit {
            return Ok(buf.len()); // drop silently; keep the app running
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        if self.written >= self.limit {
            let _ = self.file.write_all(b"\n[log capped for this run]\n");
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

pub fn run() {
    // Initialize tracing: stdout, plus a log file, since a launched app's
    // stdout goes nowhere (~/Library/Logs/Blah³/blah3.log).
    let log_file = dirs::home_dir()
        .map(|home| home.join("Library/Logs/Blah³"))
        .and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join("blah3.log");
            // Start fresh when the log grows past 5 MB.
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > 5_000_000) {
                let _ = std::fs::rename(&path, dir.join("blah3.old.log"));
            }
            // Owner-only: logs can name apps and files the user worked in.
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let file = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&path).ok()?;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            let already = file.metadata().map(|m| m.len()).unwrap_or(0);
            Some(CappedLog { file, written: already, limit: LOG_CAP_BYTES })
        });
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "blah3=debug,info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .with(log_file.map(|file| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file))
        }))
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
            // Menu bar icon with its menu
            tray::create(app.handle())?;
            overlay::setup_overlay(app.handle());

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
                    if engines::stt::is_available(&model_path) {
                        match engines::stt::preload(&model_path.to_string_lossy()) {
                            Ok(_) => tracing::info!("Speech model preloaded"),
                            Err(e) => tracing::warn!("Speech model preload failed: {}", e),
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
            dictionary::get_dictionary,
            dictionary::save_dictionary,
            dictionary::preview_dictionary,
            dictionary::fluidvoice_dictionary_available,
            dictionary::import_fluidvoice_dictionary,
            commands::tts::speak_text,
            commands::tts::stop_speaking,
            commands::tts::set_tts_speed,
            overlay::preview_overlay,
            overlay::reset_reading_overlay_position,
            history::get_history,
            history::delete_history_entry,
            history::clear_history,
            history::copy_history_entry,
            history::paste_history_entry,
            sounds::list_system_sounds,
            sounds::preview_sound,
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
