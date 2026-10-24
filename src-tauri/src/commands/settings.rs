use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::hardware::{HardwareDetector, HardwareProfile};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub stt_hotkey: String,
    pub tts_hotkey: String,
    pub stt_model: String,
    pub tts_voice: String,
    pub tts_speed: f32,
    /// "kokoro" or "phonon"
    #[serde(default = "default_tts_engine")]
    pub tts_engine: String,
    pub auto_paste: bool,
    pub launch_at_login: bool,
    pub menu_bar_mode: bool,
    // Silence detection settings
    pub silence_detection_enabled: bool,
    pub silence_threshold: f32,
    pub silence_duration: f32,
    // Onboarding
    pub onboarding_completed: bool,
    // Overlay (dictation + read-aloud). Strings are parsed leniently in
    // `overlay.rs`, so an unknown value falls back instead of failing the load.
    /// "top-center" | "top-left" | "top-right" | "bottom-center" | "bottom-left" | "bottom-right"
    pub overlay_position: String,
    /// "compact" | "regular" | "large"
    pub overlay_size: String,
    /// Transcript lines shown while dictating (2-8).
    pub overlay_max_lines: u8,
    /// Dictation overlay ignores the mouse entirely.
    pub overlay_click_through: bool,
    /// Nudge from the anchored position, in logical px (right/down positive).
    pub overlay_offset_x: i32,
    pub overlay_offset_y: i32,
    /// Where the user dragged the read-aloud pill (logical points of the
    /// monitor it was on). Only used while `reading_overlay_custom` is set.
    pub reading_overlay_x: i32,
    pub reading_overlay_y: i32,
    pub reading_overlay_custom: bool,
}

fn default_tts_engine() -> String {
    "kokoro".to_string()
}

// Off: dictation runs until the hotkey is pressed again. Pauses to think
// should not end a recording.
fn default_silence_enabled() -> bool {
    false
}

fn default_silence_threshold() -> f32 {
    0.01
}

fn default_silence_duration() -> f32 {
    1.5
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            stt_hotkey: crate::right_option::RIGHT_OPTION_HOTKEY.to_string(),
            tts_hotkey: "Option+S".to_string(),
            stt_model: "ggml-base.en.bin".to_string(),
            tts_voice: "af_heart".to_string(),
            tts_speed: 1.0,
            tts_engine: default_tts_engine(),
            auto_paste: true,
            launch_at_login: false,
            menu_bar_mode: true,
            silence_detection_enabled: default_silence_enabled(),
            silence_threshold: default_silence_threshold(),
            silence_duration: default_silence_duration(),
            onboarding_completed: false,
            overlay_position: "top-center".to_string(),
            overlay_size: "regular".to_string(),
            overlay_max_lines: 4,
            overlay_click_through: true,
            overlay_offset_x: 0,
            overlay_offset_y: 0,
            reading_overlay_x: 0,
            reading_overlay_y: 0,
            reading_overlay_custom: false,
        }
    }
}

/// Parse settings JSON. Missing fields fall back to defaults; malformed JSON
/// yields a clear error.
fn parse_settings(content: &str) -> Result<AppSettings, String> {
    serde_json::from_str(content).map_err(|e| format!("Failed to parse settings file: {}", e))
}

/// Read-modify-write the settings file. Used by backend-initiated changes
/// (the dragged reading-overlay position) so they never go through a stale
/// copy held by the Settings window.
pub fn modify_settings(change: impl FnOnce(&mut AppSettings)) -> Result<(), String> {
    let mut settings = get_settings()?;
    change(&mut settings);
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    write_atomic(&get_settings_path(), &content)
        .map_err(|e| format!("Failed to write settings file: {}", e))
}

/// Write `content` to `path` atomically (temp file in the same directory, then rename).
fn write_atomic(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let settings_path = get_settings_path();

    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("Failed to read settings file: {}", e))?;
        parse_settings(&content)
    } else {
        Ok(AppSettings::default())
    }
}

#[tauri::command]
pub fn update_settings(app: tauri::AppHandle, mut settings: AppSettings) -> Result<(), String> {
    // Reject hotkeys that can't be registered before touching disk.
    crate::hotkeys::validate_hotkey("Dictation hotkey", &settings.stt_hotkey, true)?;
    crate::hotkeys::validate_hotkey("Read aloud hotkey", &settings.tts_hotkey, false)?;
    if settings.stt_hotkey.trim().eq_ignore_ascii_case(settings.tts_hotkey.trim()) {
        return Err("Dictation and Read aloud hotkeys must be different".to_string());
    }
    validate_model_choices(&settings, get_settings().ok().as_ref())?;

    let previous = get_settings().ok();
    let settings_path = get_settings_path();

    // The dragged pill position is owned by the backend; the Settings window
    // may hold an older copy, which must not undo a drag.
    if let Some(p) = &previous {
        settings.reading_overlay_x = p.reading_overlay_x;
        settings.reading_overlay_y = p.reading_overlay_y;
        settings.reading_overlay_custom = p.reading_overlay_custom;
    }

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {}", e))?;
    }

    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    write_atomic(&settings_path, &content)
        .map_err(|e| format!("Failed to write settings file: {}", e))?;

    tracing::info!("Settings updated");

    // Only re-register hotkeys when a hotkey actually changed; re-registering
    // on every slider tick would drop the Escape-cancel shortcut mid-recording.
    let hotkeys_changed = previous.as_ref().is_none_or(|p| {
        p.stt_hotkey != settings.stt_hotkey || p.tts_hotkey != settings.tts_hotkey
    });
    if hotkeys_changed {
        if let Err(e) = crate::hotkeys::refresh_hotkeys(&app) {
            tracing::error!("Failed to refresh hotkeys: {}", e);
            // Roll back to the previous settings so disk and live shortcuts agree,
            // then report the failure to the UI.
            if let Some(prev) = previous {
                if let Ok(prev_json) = serde_json::to_string_pretty(&prev) {
                    let _ = write_atomic(&settings_path, &prev_json);
                }
                let _ = crate::hotkeys::refresh_hotkeys(&app);
            }
            return Err(e);
        }
    }

    // Warm the newly selected Whisper model so the next dictation isn't slow.
    // (The engine cache is keyed by path, so a changed model is picked up on
    // next use regardless; this just moves the load off the hot path.)
    if previous.as_ref().is_some_and(|p| p.stt_model != settings.stt_model) {
        preload_stt_model(&settings.stt_model);
    }

    // Keep the menu bar menu's checkmarks and hotkey labels current.
    crate::tray::refresh(&app);

    Ok(())
}

/// Settings feed file paths (models/stt/<stt_model>), so only accept values
/// the app itself offers; a crafted value like "../../x" must not reach a
/// model loader.
/// Only changed values are checked, so an older saved value never locks the
/// user out of saving other settings.
fn validate_model_choices(settings: &AppSettings, previous: Option<&AppSettings>) -> Result<(), String> {
    let changed = |f: fn(&AppSettings) -> &String| previous.is_none_or(|p| f(p) != f(settings));
    if !changed(|s| &s.stt_model) && !changed(|s| &s.tts_engine) {
        return Ok(());
    }
    let registry = crate::models::registry::ModelRegistry::new();
    let is_stt_model = registry.get_all_models().iter().any(|m| {
        m.model_type == crate::commands::models::ModelType::Stt
            && m.id == settings.stt_model
            && m.id.ends_with(".bin")
    });
    if changed(|s| &s.stt_model) && !is_stt_model {
        return Err(format!("Unknown speech model: {}", settings.stt_model));
    }
    if changed(|s| &s.tts_engine) && !matches!(settings.tts_engine.as_str(), "kokoro" | "phonon") {
        return Err(format!("Unknown voice engine: {}", settings.tts_engine));
    }
    Ok(())
}

fn preload_stt_model(model: &str) {
    let path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("models")
        .join("stt")
        .join(model);
    if !path.exists() {
        return;
    }
    std::thread::spawn(move || {
        match crate::engines::whisper::get_or_load_cached(&path.to_string_lossy()) {
            Ok(_) => tracing::info!("Whisper model switched and loaded"),
            Err(e) => tracing::warn!("Whisper model preload failed: {}", e),
        }
    });
}

#[tauri::command]
pub fn get_hardware_info() -> HardwareProfile {
    HardwareDetector::detect()
}

fn get_settings_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_loads_with_defaults() {
        let s = parse_settings(r#"{"tts_speed": 1.5, "auto_paste": false}"#).unwrap();
        let d = AppSettings::default();
        assert_eq!(s.tts_speed, 1.5);
        assert!(!s.auto_paste);
        assert_eq!(s.stt_model, d.stt_model);
        assert_eq!(s.stt_hotkey, d.stt_hotkey);
        assert_eq!(s.silence_duration, d.silence_duration);
    }

    #[test]
    fn overlay_defaults_match_the_original_look() {
        let s = parse_settings("{}").unwrap();
        assert_eq!(s.overlay_position, "top-center");
        assert_eq!(s.overlay_size, "regular");
        assert_eq!(s.overlay_max_lines, 4);
        assert!(s.overlay_click_through);
        assert_eq!((s.overlay_offset_x, s.overlay_offset_y), (0, 0));
        assert!(!s.reading_overlay_custom);
        assert_eq!((s.reading_overlay_x, s.reading_overlay_y), (0, 0));
    }

    #[test]
    fn empty_object_is_all_defaults() {
        let s = parse_settings("{}").unwrap();
        assert_eq!(s.tts_hotkey, AppSettings::default().tts_hotkey);
    }

    #[test]
    fn malformed_json_errors_clearly() {
        let err = parse_settings("{not json").unwrap_err();
        assert!(err.starts_with("Failed to parse settings file"), "{err}");
    }

    #[test]
    fn atomic_write_replaces_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("b3t-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        write_atomic(&path, "{\"a\":1}").unwrap();
        write_atomic(&path, "{\"a\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":2}");
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn model_choices_are_validated_when_changed() {
        let base = AppSettings::default();
        assert!(validate_model_choices(&base, None).is_ok());

        let traversal = AppSettings { stt_model: "../../../../tmp/evil.bin".into(), ..base.clone() };
        assert!(validate_model_choices(&traversal, Some(&base)).is_err());
        let encoder = AppSettings { stt_model: "ggml-base.en-encoder.mlmodelc".into(), ..base.clone() };
        assert!(validate_model_choices(&encoder, Some(&base)).is_err());
        let small = AppSettings { stt_model: "ggml-small.en.bin".into(), ..base.clone() };
        assert!(validate_model_choices(&small, Some(&base)).is_ok());

        let engine = AppSettings { tts_engine: "../x".into(), ..base.clone() };
        assert!(validate_model_choices(&engine, Some(&base)).is_err());

        // An unchanged (older) value doesn't block saving other settings.
        let stale = AppSettings { stt_model: "ggml-removed.bin".into(), ..base.clone() };
        let stale_speed = AppSettings { tts_speed: 1.5, ..stale.clone() };
        assert!(validate_model_choices(&stale_speed, Some(&stale)).is_ok());
    }
}
