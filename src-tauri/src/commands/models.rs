use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::{download::ModelDownloader, registry::ModelRegistry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub model_type: ModelType,
    pub size_bytes: u64,
    pub size_display: String,
    pub download_url: String,
    pub status: ModelStatus,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
    Stt,
    Tts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    Available,
    Downloaded,
    Downloading,
}

#[tauri::command]
pub fn list_models() -> Vec<ModelInfo> {
    let registry = ModelRegistry::new();
    let models_dir = get_models_dir();

    registry
        .get_all_models()
        .into_iter()
        .map(|mut model| {
            let model_path = models_dir
                .join(match model.model_type {
                    ModelType::Stt => "stt",
                    ModelType::Tts => "tts",
                })
                .join(&model.id);

            model.status = if model_path.exists() {
                ModelStatus::Downloaded
            } else {
                ModelStatus::Available
            };

            model
        })
        .collect()
}

#[tauri::command]
pub async fn download_model(
    model_id: String,
    window: tauri::Window,
) -> Result<String, String> {
    tracing::info!("Downloading model: {}", model_id);

    let registry = ModelRegistry::new();
    let model = registry
        .get_model(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    let models_dir = get_models_dir();
    let type_dir = models_dir.join(match model.model_type {
        ModelType::Stt => "stt",
        ModelType::Tts => "tts",
    });

    std::fs::create_dir_all(&type_dir).map_err(|e| e.to_string())?;

    let dest_path = type_dir.join(&model_id);
    let downloader = ModelDownloader::new();

    downloader
        .download(&model.download_url, &dest_path, move |progress| {
            let _ = window.emit("model-download-progress", (&model_id, progress));
        })
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("Model downloaded: {}", model_id);
    Ok(dest_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn delete_model(model_id: String) -> Result<(), String> {
    tracing::info!("Deleting model: {}", model_id);

    let registry = ModelRegistry::new();
    let model = registry
        .get_model(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    let models_dir = get_models_dir();
    let model_path = models_dir
        .join(match model.model_type {
            ModelType::Stt => "stt",
            ModelType::Tts => "tts",
        })
        .join(&model_id);

    if model_path.exists() {
        if model_path.is_dir() {
            std::fs::remove_dir_all(&model_path).map_err(|e| e.to_string())?;
        } else {
            std::fs::remove_file(&model_path).map_err(|e| e.to_string())?;
        }
        tracing::info!("Model deleted: {}", model_id);
    }

    Ok(())
}

#[tauri::command]
pub fn get_model_status(model_id: String) -> Result<ModelStatus, String> {
    let registry = ModelRegistry::new();
    let model = registry
        .get_model(&model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    let models_dir = get_models_dir();
    let model_path = models_dir
        .join(match model.model_type {
            ModelType::Stt => "stt",
            ModelType::Tts => "tts",
        })
        .join(&model_id);

    Ok(if model_path.exists() {
        ModelStatus::Downloaded
    } else {
        ModelStatus::Available
    })
}

fn get_models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("models")
}
