use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Emitter;

use crate::engines::phonon;
use crate::models::{
    download::{extract_zip, DownloadProgress, ModelDownloader},
    registry::ModelRegistry,
};

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

            model.status = status_on_disk(&model, &model_path);
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
    let model_id_for_progress = model_id.clone();

    // Check if this is a CoreML model (zip file that needs extraction)
    let is_coreml = model_id.ends_with(".mlmodelc") && model.download_url.ends_with(".zip");

    if model_id == phonon::MODEL_ID {
        // Several files into one directory. Files already complete are
        // skipped, so a retry resumes where the last attempt failed.
        std::fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
        let total = phonon::total_size();
        let mut done = 0u64;
        for (name, size) in phonon::FILES {
            let file_path = dest_path.join(name);
            let complete = std::fs::metadata(&file_path).map(|m| m.len() == size).unwrap_or(false);
            if !complete {
                let window = window.clone();
                let id = model_id_for_progress.clone();
                downloader
                    .download(&phonon::file_url(name), &file_path, move |p| {
                        let downloaded = done + p.downloaded;
                        let progress = DownloadProgress {
                            downloaded,
                            total,
                            percentage: (downloaded as f64 / total as f64 * 100.0).min(100.0) as u8,
                        };
                        let _ = window.emit("model-download-progress", (&id, progress));
                    })
                    .await
                    .map_err(|e| format!("Failed to download {}: {}", name, e))?;
            }
            done += size;
        }
        let missing = phonon::missing_files(&dest_path);
        if !missing.is_empty() {
            return Err(format!("Phonon download incomplete: {}", missing.join(", ")));
        }
        tracing::info!("Phonon model downloaded");
    } else if is_coreml {
        // Download to a temp zip file
        let zip_path = type_dir.join(format!("{}.zip", model_id));

        downloader
            .download(&model.download_url, &zip_path, move |progress| {
                let _ = window.emit("model-download-progress", (&model_id_for_progress, progress));
            })
            .await
            .map_err(|e| e.to_string())?;

        // Extract the zip to the destination directory
        tracing::info!("Extracting CoreML model: {} -> {:?}", model_id, dest_path);
        extract_zip(&zip_path, &dest_path).map_err(|e| format!("Failed to extract: {}", e))?;

        // Clean up the zip file
        if let Err(e) = std::fs::remove_file(&zip_path) {
            tracing::warn!("Failed to remove temp zip file: {}", e);
        }

        tracing::info!("CoreML model extracted: {}", model_id);
    } else {
        // Regular file download
        downloader
            .download(&model.download_url, &dest_path, move |progress| {
                let _ = window.emit("model-download-progress", (&model_id_for_progress, progress));
            })
            .await
            .map_err(|e| e.to_string())?;

        tracing::info!("Model downloaded: {}", model_id);
    }

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

    Ok(status_on_disk(&model, &model_path))
}

/// A model counts as downloaded only when its files are really there: a
/// failed download used to leave a 15-byte error page that showed as
/// "Downloaded". Single files must be at least half the expected size.
fn status_on_disk(model: &ModelInfo, path: &std::path::Path) -> ModelStatus {
    let complete = if model.id == phonon::MODEL_ID {
        phonon::missing_files(path).is_empty()
    } else if path.is_dir() {
        true
    } else {
        std::fs::metadata(path).map(|m| m.len() >= model.size_bytes / 2).unwrap_or(false)
    };
    if complete {
        ModelStatus::Downloaded
    } else {
        ModelStatus::Available
    }
}

fn get_models_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("models")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, size_bytes: u64) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: id.into(),
            model_type: ModelType::Tts,
            size_bytes,
            size_display: String::new(),
            download_url: String::new(),
            status: ModelStatus::Available,
            description: String::new(),
        }
    }

    #[test]
    fn error_page_is_not_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kokoro-v1.0.onnx");
        let m = model("kokoro-v1.0.onnx", 325_532_387);
        assert_eq!(status_on_disk(&m, &path), ModelStatus::Available);
        std::fs::write(&path, b"Entry not found").unwrap();
        assert_eq!(status_on_disk(&m, &path), ModelStatus::Available);
        std::fs::File::create(&path).unwrap().set_len(325_532_387).unwrap();
        assert_eq!(status_on_disk(&m, &path), ModelStatus::Downloaded);
    }

    #[test]
    fn phonon_needs_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(phonon::MODEL_ID);
        std::fs::create_dir_all(&path).unwrap();
        let m = model(phonon::MODEL_ID, phonon::total_size());
        // An existing directory alone is not enough.
        assert_eq!(status_on_disk(&m, &path), ModelStatus::Available);
        for (name, size) in phonon::FILES {
            std::fs::File::create(path.join(name)).unwrap().set_len(size).unwrap();
        }
        assert_eq!(status_on_disk(&m, &path), ModelStatus::Downloaded);
    }

    #[test]
    fn registry_lists_phonon_and_working_kokoro_urls() {
        let models = ModelRegistry::new().get_all_models();
        assert!(models.iter().any(|m| m.id == phonon::MODEL_ID && m.model_type == ModelType::Tts));
        for id in ["kokoro-v1.0.onnx", "voices-v1.0.bin"] {
            let m = models.iter().find(|m| m.id == id).unwrap();
            // The old Hugging Face path 404s and was saved as the model.
            assert!(!m.download_url.contains("onnx-community"), "{}", m.download_url);
        }
    }
}
