//! NVIDIA Parakeet TDT 0.6B v2 (English) speech-to-text, run on-device with
//! ONNX Runtime through the `parakeet-rs` crate. Model weights are CC-BY-4.0
//! (NVIDIA); the int8 ONNX export is istupakov's.

use anyhow::{anyhow, Result};
use parakeet_rs::{ExecutionConfig, ExecutionProvider, ParakeetTDT, Transcriber};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Model id in the registry/settings and the directory name under `models/stt/`.
pub const MODEL_ID: &str = "parakeet-tdt-0.6b-v2";

/// Hugging Face revision of `istupakov/parakeet-tdt-0.6b-v2-onnx` the files
/// are pinned to.
pub const REVISION: &str = "0bbb45a3365852604aef28b538a8f066f4ccaa85";

/// Files the runtime needs, with exact sizes and SHA-256 at `REVISION`
/// (LFS hashes from Hugging Face; `vocab.txt` hashed from the pinned file).
/// Only the int8 export is used (fp32 is 2.4 GB).
pub const FILES: [(&str, u64, &str); 3] = [
    ("vocab.txt", 9_384, "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d"),
    ("decoder_joint-model.int8.onnx", 8_998_286, "a449f49acd68979d418651dd2dcb737cc0f1bf0225e009e29ee326354edbf7d3"),
    ("encoder-model.int8.onnx", 652_184_014, "3e0581fda6ab843888b51e56d7ee78b6d5bc3237ec113af1f732d1d5286aa155"),
];

pub fn file_url(name: &str) -> String {
    format!("https://huggingface.co/istupakov/parakeet-tdt-0.6b-v2-onnx/resolve/{REVISION}/{name}")
}

pub fn total_size() -> u64 {
    FILES.iter().map(|(_, size, _)| size).sum()
}

/// Names of files that are missing or the wrong size.
pub fn missing_files(model_dir: &Path) -> Vec<&'static str> {
    FILES
        .iter()
        .filter(|(name, size, _)| {
            std::fs::metadata(model_dir.join(name)).map(|m| m.len() != *size).unwrap_or(true)
        })
        .map(|(name, _, _)| *name)
        .collect()
}

/// Which ONNX Runtime provider runs the encoder. CPU int8 by default; the
/// benchmark can compare CoreML with `PARAKEET_EP=coreml`.
fn execution_config() -> Result<ExecutionConfig> {
    let environment = parakeet_rs::ort::init()
        .build()
        .map_err(|e| anyhow!("Failed to start ONNX Runtime: {}", e))?;
    let provider = match std::env::var("PARAKEET_EP").as_deref() {
        Ok("coreml") => ExecutionProvider::CoreML,
        _ => ExecutionProvider::Cpu,
    };
    let threads = std::env::var("PARAKEET_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8));
    Ok(ExecutionConfig::new()
        .with_environment(environment)
        .with_execution_provider(provider)
        .with_intra_threads(threads))
}

pub struct ParakeetEngine {
    model: Mutex<ParakeetTDT>,
}

impl ParakeetEngine {
    pub fn new(model_dir: &Path) -> Result<Self> {
        let missing = missing_files(model_dir);
        if !missing.is_empty() {
            return Err(anyhow!(
                "Parakeet model files missing or incomplete ({}). Download Parakeet TDT from the Models tab.",
                missing.join(", ")
            ));
        }
        tracing::info!("Loading Parakeet model from: {:?}", model_dir);
        let model = ParakeetTDT::from_pretrained(model_dir, Some(execution_config()?))
            .map_err(|e| anyhow!("Failed to load Parakeet model: {}", e))?;
        tracing::info!("Parakeet model loaded");
        Ok(Self { model: Mutex::new(model) })
    }

    /// `audio` is mono 16 kHz f32.
    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        // Under ~0.1 s there is nothing to recognise (and the feature
        // extractor has no frames to work with).
        if audio.len() < 1_600 {
            return Ok(String::new());
        }
        let mut model = self.model.lock().unwrap_or_else(|p| p.into_inner());
        let result = model
            .transcribe_samples(audio.to_vec(), 16_000, 1, None)
            .map_err(|e| anyhow!("Transcription failed: {}", e))?;
        Ok(crate::engines::postprocess::finalize(&result.text))
    }
}

type CachedEngine = Option<(PathBuf, Arc<ParakeetEngine>)>;
static ENGINE_CACHE: OnceLock<Mutex<CachedEngine>> = OnceLock::new();

/// Cached loader, like `whisper::get_or_load_cached`: loading takes seconds,
/// so the engine stays alive between dictations.
pub fn get_or_load_cached(model_dir: &Path) -> Result<Arc<ParakeetEngine>> {
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((dir, engine)) = guard.as_ref() {
        if dir == model_dir {
            return Ok(Arc::clone(engine));
        }
    }
    let engine = Arc::new(ParakeetEngine::new(model_dir)?);
    *guard = Some((model_dir.to_path_buf(), Arc::clone(&engine)));
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn urls_are_pinned_to_a_revision() {
        let url = file_url("vocab.txt");
        assert!(url.contains(REVISION) && url.ends_with("/vocab.txt"));
        assert!(!url.contains("/main/"));
    }

    #[test]
    fn missing_files_detects_absent_and_truncated() {
        let dir = tempdir().unwrap();
        assert_eq!(missing_files(dir.path()).len(), FILES.len());
        std::fs::write(dir.path().join("vocab.txt"), b"Entry not found").unwrap();
        assert!(missing_files(dir.path()).contains(&"vocab.txt"));
        for (name, size, _) in FILES {
            std::fs::File::create(dir.path().join(name)).unwrap().set_len(size).unwrap();
        }
        assert!(missing_files(dir.path()).is_empty());
    }

    #[test]
    fn new_reports_missing_files() {
        let dir = tempdir().unwrap();
        let err = ParakeetEngine::new(dir.path()).err().unwrap().to_string();
        assert!(err.contains("Models tab"), "{err}");
    }

    #[test]
    fn total_size_is_about_661_mb() {
        assert!((660_000_000..662_000_000).contains(&total_size()));
    }
}
