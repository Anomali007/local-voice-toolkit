//! Phonon 31m (Gradium) text-to-speech: a 31M-parameter English model,
//! Apache-2.0 weights, run on-device via the `ptts` crate.

use anyhow::{anyhow, Result};
use ptts::checkpoint::{Checkpoint, ResolveOptions};
use ptts::preprocess::Lang;
use ptts::synth::{DeviceKind, Quant, Synth};
use std::path::{Path, PathBuf};

use super::VoiceInfo;

/// Model id in the registry and the directory name under `models/tts/`.
pub const MODEL_ID: &str = "phonon-31m-en";

/// Hugging Face revision the files are pinned to.
pub const REVISION: &str = "8c64b55ec301728e2d1695bfea87a08a267d0c2d";

/// Files the checkpoint needs, with their exact sizes at `REVISION`.
pub const FILES: [(&str, u64); 4] = [
    ("config.json", 2_851),
    ("tokenizer.json", 277_967),
    ("model.safetensors", 102_823_920),
    ("default-voice.safetensors", 256_136),
];

pub fn file_url(name: &str) -> String {
    format!("https://huggingface.co/gradium/{MODEL_ID}/resolve/{REVISION}/{name}")
}

pub fn total_size() -> u64 {
    FILES.iter().map(|(_, size)| size).sum()
}

/// Names of files that are missing or the wrong size.
pub fn missing_files(model_dir: &Path) -> Vec<&'static str> {
    FILES
        .iter()
        .filter(|(name, size)| {
            std::fs::metadata(model_dir.join(name)).map(|m| m.len() != *size).unwrap_or(true)
        })
        .map(|(name, _)| *name)
        .collect()
}

pub struct PhononEngine {
    synth: Synth,
    model_dir: PathBuf,
}

impl PhononEngine {
    pub fn new(model_dir: PathBuf) -> Result<Self> {
        let missing = missing_files(&model_dir);
        if !missing.is_empty() {
            return Err(anyhow!(
                "Phonon model files missing or incomplete ({}). Download Phonon 31m from the Models tab.",
                missing.join(", ")
            ));
        }

        tracing::info!("Loading Phonon TTS model from: {:?}", model_dir);
        let checkpoint = Checkpoint::resolve(&model_dir, ResolveOptions { quant: Quant::F32, weights: None })
            .map_err(|e| anyhow!("Failed to read Phonon checkpoint: {}", e))?;

        let mut synth = match checkpoint.builder(Lang::En).device(DeviceKind::Metal).build() {
            Ok(synth) => synth,
            Err(e) => {
                tracing::warn!("Phonon on Metal failed ({}), falling back to CPU", e);
                checkpoint
                    .builder(Lang::En)
                    .device(DeviceKind::Cpu)
                    .build()
                    .map_err(|e| anyhow!("Failed to load Phonon TTS: {}", e))?
            }
        };
        checkpoint.register_voices(&mut synth);
        tracing::info!("Phonon TTS loaded (voices: {:?})", synth.voices());

        Ok(Self { synth, model_dir })
    }

    pub fn sample_rate(&self) -> u32 {
        self.synth.sample_rate()
    }

    pub fn synthesize(&self, text: &str) -> Result<Vec<f32>> {
        self.synth.say(text).map_err(|e| anyhow!("Phonon synthesis failed: {}", e))
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

/// Phonon 31m ships one voice per checkpoint.
pub fn voices() -> Vec<VoiceInfo> {
    vec![VoiceInfo { id: "default".into(), name: "Phonon".into(), language: "en-US".into() }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn urls_are_pinned_to_a_revision() {
        let url = file_url("model.safetensors");
        assert!(url.contains(REVISION) && url.ends_with("/model.safetensors"));
        assert!(!url.contains("/main/"));
    }

    #[test]
    fn missing_files_detects_absent_and_truncated() {
        let dir = tempdir().unwrap();
        assert_eq!(missing_files(dir.path()).len(), FILES.len());

        // A 15-byte "Entry not found" body must not count as a model file.
        std::fs::write(dir.path().join("config.json"), b"Entry not found").unwrap();
        assert!(missing_files(dir.path()).contains(&"config.json"));

        for (name, size) in FILES {
            let f = std::fs::File::create(dir.path().join(name)).unwrap();
            f.set_len(size).unwrap();
        }
        assert!(missing_files(dir.path()).is_empty());
    }

    #[test]
    fn new_reports_missing_files() {
        let dir = tempdir().unwrap();
        let err = PhononEngine::new(dir.path().to_path_buf()).err().unwrap().to_string();
        assert!(err.contains("Models tab"), "{err}");
    }

    #[test]
    fn total_size_is_about_100_mb() {
        assert!((100_000_000..110_000_000).contains(&total_size()));
    }
}

/// Real-model smoke test. Needs the checkpoint files on disk:
/// `PHONON_DIR=/path/to/phonon-31m-en cargo test --release phonon_real -- --ignored --nocapture`
#[cfg(test)]
mod real_model {
    use super::*;

    #[test]
    #[ignore]
    fn phonon_real_synthesis_and_speed() {
        let dir = std::env::var("PHONON_DIR").expect("set PHONON_DIR");
        let t0 = std::time::Instant::now();
        let engine = PhononEngine::new(dir.into()).unwrap();
        let load = t0.elapsed();
        let t1 = std::time::Instant::now();
        let pcm = engine.synthesize("Testing Phonon inside Blah cubed. It is ten thirty p.m.").unwrap();
        let synth = t1.elapsed();
        let sr = engine.sample_rate();
        let secs = pcm.len() as f32 / sr as f32;
        let rms = (pcm.iter().map(|v| v * v).sum::<f32>() / pcm.len() as f32).sqrt();
        println!("load {load:?}, synth {synth:?} for {secs:.2}s audio at {sr} Hz, rms {rms:.4}");
        assert!(secs > 1.5 && rms > 0.01, "audio looks empty");

        let fast = crate::audio::stretch::time_stretch(&pcm, sr, 1.5);
        let ratio = pcm.len() as f32 / fast.len() as f32;
        println!("1.5x stretch: {:.2}s -> {:.2}s (ratio {ratio:.3})", secs, fast.len() as f32 / sr as f32);
        assert!((ratio - 1.5).abs() < 0.02);
        crate::audio::processing::write_wav(std::path::Path::new("/tmp/phonon-real-1x.wav"), &pcm, sr).unwrap();
        crate::audio::processing::write_wav(std::path::Path::new("/tmp/phonon-real-1.5x.wav"), &fast, sr).unwrap();
    }
}
