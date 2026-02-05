use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::hardware::{HardwareDetector, HardwareProfile};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub stt_hotkey: String,
    pub tts_hotkey: String,
    pub stt_model: String,
    pub tts_voice: String,
    pub tts_speed: f32,
    pub auto_paste: bool,
    pub launch_at_login: bool,
    pub menu_bar_mode: bool,
    // Silence detection settings
    #[serde(default = "default_silence_enabled")]
    pub silence_detection_enabled: bool,
    #[serde(default = "default_silence_threshold")]
    pub silence_threshold: f32,
    #[serde(default = "default_silence_duration")]
    pub silence_duration: f32,
    // Onboarding
    #[serde(default)]
    pub onboarding_completed: bool,
}

fn default_silence_enabled() -> bool {
    true
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
            stt_hotkey: "CommandOrControl+Shift+D".to_string(),
            tts_hotkey: "CommandOrControl+Shift+S".to_string(),
            stt_model: "ggml-base.en.bin".to_string(),
            tts_voice: "af_heart".to_string(),
            tts_speed: 1.0,
            auto_paste: true,
            launch_at_login: false,
            menu_bar_mode: true,
            silence_detection_enabled: default_silence_enabled(),
            silence_threshold: default_silence_threshold(),
            silence_duration: default_silence_duration(),
            onboarding_completed: false,
        }
    }
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let settings_path = get_settings_path();

    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(AppSettings::default())
    }
}

#[tauri::command]
pub fn update_settings(settings: AppSettings) -> Result<(), String> {
    let settings_path = get_settings_path();

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&settings_path, content).map_err(|e| e.to_string())?;

    tracing::info!("Settings updated");
    Ok(())
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
