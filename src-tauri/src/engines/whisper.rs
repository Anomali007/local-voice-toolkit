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
        let dictionary = crate::dictionary::load();
        if let Some(prompt) = crate::dictionary::vocabulary_prompt(&dictionary) {
            params.set_initial_prompt(&prompt.replace('\0', ""));
        }

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

        Ok(crate::dictionary::apply(text.trim(), &dictionary))
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
        let dictionary = crate::dictionary::load();
        if let Some(prompt) = crate::dictionary::vocabulary_prompt(&dictionary) {
            params.set_initial_prompt(&prompt.replace('\0', ""));
        }
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

        Ok(crate::dictionary::apply(text.trim(), &dictionary))
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

#[cfg(test)]
mod real_models {
    use super::*;

    /// `cargo test --release --lib whisper_real -- --ignored --nocapture`
    /// Times each downloaded model on a known WAV (WHISPER_WAV, any rate).
    #[test]
    #[ignore]
    fn whisper_real_benchmark() {
        let wav = std::env::var("WHISPER_WAV").expect("set WHISPER_WAV");
        let (samples, rate) = crate::audio::processing::read_wav(std::path::Path::new(&wav)).unwrap();
        let audio = crate::audio::capture::convert_rate(&samples, rate, 16_000);
        let dir = dirs::data_dir().unwrap().join("com.blahcubed.app/models/stt");
        for model in ["ggml-base.en.bin", "ggml-small.en.bin"] {
            let path = dir.join(model);
            if !path.exists() {
                continue;
            }
            let t0 = std::time::Instant::now();
            let engine = WhisperEngine::new(&path.to_string_lossy()).unwrap();
            let load = t0.elapsed();
            let _ = engine.transcribe(&audio).unwrap(); // warm-up
            let t1 = std::time::Instant::now();
            let text = engine.transcribe(&audio).unwrap();
            println!("{model}: load {load:?}, transcribe {:?} for {:.1}s audio -> {text}", t1.elapsed(), audio.len() as f32 / 16_000.0);
        }
    }
}
