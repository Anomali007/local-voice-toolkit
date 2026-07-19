use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex, OnceLock};
use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
};

use super::{ModelInfo, SpeechToText};

/// (model path, loaded engine) pair kept alive between dictations.
type CachedEngine = Option<(String, Arc<WhisperEngine>)>;

/// Process-wide cache of the loaded Whisper model, keyed by model path.
/// Loading a model (especially with CoreML compilation) can take seconds,
/// so we keep the last-used engine alive across dictations.
static ENGINE_CACHE: OnceLock<Mutex<CachedEngine>> = OnceLock::new();

/// Get the cached engine for `model_path`, loading (and caching) it if needed.
/// Switching models replaces the cached engine.
pub fn get_or_load_cached(model_path: &str) -> Result<Arc<WhisperEngine>> {
    let cache = ENGINE_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some((cached_path, engine)) = guard.as_ref() {
        if cached_path == model_path {
            return Ok(Arc::clone(engine));
        }
    }

    let engine = Arc::new(WhisperEngine::new(model_path)?);
    *guard = Some((model_path.to_string(), Arc::clone(&engine)));
    Ok(engine)
}

pub struct WhisperEngine {
    ctx: WhisperContext,
    model_path: String,
}

impl WhisperEngine {
    pub fn new(model_path: &str) -> Result<Self> {
        tracing::info!("Loading Whisper model from: {}", model_path);

        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| anyhow!("Failed to load Whisper model: {}", e))?;

        tracing::info!("Whisper model loaded successfully");

        Ok(Self {
            ctx,
            model_path: model_path.to_string(),
        })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_single_segment(false);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("Failed to create Whisper state: {}", e))?;

        state
            .full(params, audio)
            .map_err(|e| anyhow!("Transcription failed: {}", e))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| anyhow!("Failed to get segment count: {}", e))?;

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state
                .full_get_segment_text(i)
                .map_err(|e| anyhow!("Failed to get segment {}: {}", i, e))?;
            text.push_str(&segment);
        }

        Ok(text.trim().to_string())
    }

    /// Transcribe audio with a callback fired as each segment is decoded.
    /// The callback receives `SegmentCallbackData` with segment text and timestamps.
    pub fn transcribe_streaming<F>(&self, audio: &[f32], on_segment: F) -> Result<String>
    where
        F: FnMut(SegmentCallbackData) + 'static,
    {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_single_segment(false);
        params.set_segment_callback_safe_lossy(on_segment);

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow!("Failed to create Whisper state: {}", e))?;

        state
            .full(params, audio)
            .map_err(|e| anyhow!("Transcription failed: {}", e))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| anyhow!("Failed to get segment count: {}", e))?;

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state
                .full_get_segment_text(i)
                .map_err(|e| anyhow!("Failed to get segment {}: {}", i, e))?;
            text.push_str(&segment);
        }

        Ok(text.trim().to_string())
    }
}

impl SpeechToText for WhisperEngine {
    fn transcribe(&self, audio: &[f32]) -> Result<String> {
        self.transcribe(audio)
    }

    fn model_info(&self) -> ModelInfo {
        let size = std::fs::metadata(&self.model_path)
            .map(|m| m.len())
            .unwrap_or(0);

        ModelInfo {
            name: self.model_path.clone(),
            size_bytes: size,
            loaded: true,
        }
    }
}
