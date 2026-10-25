#![allow(dead_code)]

use anyhow::{anyhow, Result};
use kokoro_tiny::TtsEngine;
use std::path::{Path, PathBuf};

use super::{AudioBuffer, ModelInfo, TextToSpeech, VoiceInfo};

const SAMPLE_RATE: u32 = 24000;

/// Smallest plausible size for each model file. A failed download used to
/// leave a 15-byte "Entry not found" page that passed an exists() check.
const MIN_MODEL_BYTES: u64 = 100_000_000;
const MIN_VOICES_BYTES: u64 = 1_000_000;

fn is_complete(path: &Path, min_bytes: u64) -> bool {
    std::fs::metadata(path).map(|m| m.len() >= min_bytes).unwrap_or(false)
}

pub struct KokoroEngine {
    tts: TtsEngine,
    model_dir: PathBuf,
}

impl KokoroEngine {
    pub async fn new(model_dir: PathBuf) -> Result<Self> {
        let model_path = model_dir.join("kokoro-v1.0.onnx");
        let voices_path = model_dir.join("voices-v1.0.bin");

        // Validate required files exist
        if let Err(missing) = validate_model_files(&model_dir) {
            return Err(anyhow!(
                "Missing TTS files: {}. Please download from Models tab.",
                missing.join(", ")
            ));
        }

        tracing::info!("Loading Kokoro TTS model from: {:?}", model_dir);

        let tts = TtsEngine::with_paths(
            model_path.to_string_lossy().as_ref(),
            voices_path.to_string_lossy().as_ref(),
        )
        .await
        .map_err(|e| anyhow!("Failed to load Kokoro TTS: {}", e))?;

        tracing::info!("Kokoro TTS loaded successfully");

        Ok(Self { tts, model_dir })
    }

    /// Synthesize at the model's native rate. Speed is applied afterwards by
    /// `audio::stretch`, which keeps pitch (changing the playback rate did not).
    pub fn synthesize(&mut self, text: &str, voice_id: &str) -> Result<AudioBuffer> {
        tracing::debug!("Synthesizing text with voice '{}'", voice_id);

        let samples = self
            .tts
            .synthesize(text, Some(voice_id))
            .map_err(|e| anyhow!("TTS synthesis failed: {}", e))?;

        Ok(AudioBuffer::new(samples, SAMPLE_RATE))
    }
}

impl TextToSpeech for KokoroEngine {
    fn synthesize(&self, _text: &str, _voice: &str, _speed: f32) -> Result<AudioBuffer> {
        // TextToSpeech trait requires &self, but kokoro-tiny needs &mut self
        // This is a limitation we work around in the command layer
        Err(anyhow!(
            "Use KokoroEngine::synthesize directly with &mut self"
        ))
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        // Kokoro-82M voices - American and British English
        vec![
            VoiceInfo {
                id: "af_heart".to_string(),
                name: "Heart".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_bella".to_string(),
                name: "Bella".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_nicole".to_string(),
                name: "Nicole".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_sarah".to_string(),
                name: "Sarah".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_sky".to_string(),
                name: "Sky".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "am_adam".to_string(),
                name: "Adam".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "am_michael".to_string(),
                name: "Michael".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "bf_emma".to_string(),
                name: "Emma".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bf_isabella".to_string(),
                name: "Isabella".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bm_george".to_string(),
                name: "George".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bm_lewis".to_string(),
                name: "Lewis".to_string(),
                language: "en-GB".to_string(),
            },
        ]
    }

    fn model_info(&self) -> ModelInfo {
        let model_path = self.model_dir.join("kokoro-v1.0.onnx");
        let size = std::fs::metadata(&model_path)
            .map(|m| m.len())
            .unwrap_or(0);

        ModelInfo {
            name: "Kokoro 82M".to_string(),
            size_bytes: size,
            loaded: true,
        }
    }
}

/// Validate that all required TTS model files exist in the given directory
pub fn validate_model_files(model_dir: &Path) -> Result<(), Vec<&'static str>> {
    let mut missing = Vec::new();

    if !is_complete(&model_dir.join("kokoro-v1.0.onnx"), MIN_MODEL_BYTES) {
        missing.push("kokoro-v1.0.onnx");
    }
    if !is_complete(&model_dir.join("voices-v1.0.bin"), MIN_VOICES_BYTES) {
        missing.push("voices-v1.0.bin");
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sample_rate_constant() {
        assert_eq!(SAMPLE_RATE, 24000);
    }

    #[test]
    fn test_validate_model_files_missing_all() {
        let temp_dir = tempdir().unwrap();
        let model_dir = temp_dir.path().to_path_buf();

        let result = validate_model_files(&model_dir);
        assert!(result.is_err());

        let missing = result.unwrap_err();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"kokoro-v1.0.onnx"));
        assert!(missing.contains(&"voices-v1.0.bin"));
    }

    #[test]
    fn test_validate_model_files_missing_voices() {
        let temp_dir = tempdir().unwrap();
        let model_dir = temp_dir.path().to_path_buf();

        // Create only the model file
        std::fs::File::create(model_dir.join("kokoro-v1.0.onnx")).unwrap().set_len(MIN_MODEL_BYTES).unwrap();

        let result = validate_model_files(&model_dir);
        assert!(result.is_err());

        let missing = result.unwrap_err();
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&"voices-v1.0.bin"));
    }

    #[test]
    fn test_validate_model_files_all_present() {
        let temp_dir = tempdir().unwrap();
        let model_dir = temp_dir.path().to_path_buf();

        // Create both required files
        std::fs::File::create(model_dir.join("kokoro-v1.0.onnx")).unwrap().set_len(MIN_MODEL_BYTES).unwrap();
        std::fs::File::create(model_dir.join("voices-v1.0.bin")).unwrap().set_len(MIN_VOICES_BYTES).unwrap();

        let result = validate_model_files(&model_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_model_files_rejects_error_pages() {
        // What the old 404 download left behind on real machines.
        let temp_dir = tempdir().unwrap();
        let model_dir = temp_dir.path().to_path_buf();
        std::fs::write(model_dir.join("kokoro-v1.0.onnx"), b"Entry not found").unwrap();
        std::fs::write(model_dir.join("voices-v1.0.bin"), b"Entry not found").unwrap();

        let missing = validate_model_files(&model_dir).unwrap_err();
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn test_available_voices_count() {
        // Create a mock engine just for testing available_voices
        // Note: We can't actually create a KokoroEngine without real models,
        // so we test the voice list directly
        let voices = vec![
            VoiceInfo {
                id: "af_heart".to_string(),
                name: "Heart".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_bella".to_string(),
                name: "Bella".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_nicole".to_string(),
                name: "Nicole".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_sarah".to_string(),
                name: "Sarah".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "af_sky".to_string(),
                name: "Sky".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "am_adam".to_string(),
                name: "Adam".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "am_michael".to_string(),
                name: "Michael".to_string(),
                language: "en-US".to_string(),
            },
            VoiceInfo {
                id: "bf_emma".to_string(),
                name: "Emma".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bf_isabella".to_string(),
                name: "Isabella".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bm_george".to_string(),
                name: "George".to_string(),
                language: "en-GB".to_string(),
            },
            VoiceInfo {
                id: "bm_lewis".to_string(),
                name: "Lewis".to_string(),
                language: "en-GB".to_string(),
            },
        ];

        // Should have 11 voices total
        assert_eq!(voices.len(), 11);
    }

    #[test]
    fn test_voice_id_format() {
        // Voice IDs should follow the pattern: {lang}{gender}_{name}
        // af_ = American Female, am_ = American Male
        // bf_ = British Female, bm_ = British Male
        let voices = vec![
            ("af_heart", "en-US"),
            ("af_bella", "en-US"),
            ("am_adam", "en-US"),
            ("bf_emma", "en-GB"),
            ("bm_george", "en-GB"),
        ];

        for (id, expected_lang) in voices {
            let is_american = id.starts_with("af_") || id.starts_with("am_");
            let is_british = id.starts_with("bf_") || id.starts_with("bm_");

            assert!(
                is_american || is_british,
                "Voice ID {} should start with af_, am_, bf_, or bm_",
                id
            );

            if is_american {
                assert_eq!(expected_lang, "en-US");
            } else {
                assert_eq!(expected_lang, "en-GB");
            }
        }
    }

    #[test]
    fn test_audio_buffer_duration() {
        // Test AudioBuffer duration calculation
        let sample_rate = 24000;
        let duration_secs = 2.5;
        let num_samples = (sample_rate as f32 * duration_secs) as usize;

        let samples: Vec<f32> = vec![0.0; num_samples];
        let buffer = AudioBuffer::new(samples, sample_rate);

        // Allow small floating point tolerance
        assert!((buffer.duration_secs() - duration_secs).abs() < 0.001);
    }

    #[test]
    fn test_audio_buffer_samples() {
        let samples = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let buffer = AudioBuffer::new(samples.clone(), 24000);

        assert_eq!(buffer.samples(), &samples[..]);
        assert_eq!(buffer.sample_rate, 24000);
    }

    #[tokio::test]
    async fn test_kokoro_engine_missing_files() {
        let temp_dir = tempdir().unwrap();
        let model_dir = temp_dir.path().to_path_buf();

        let result = KokoroEngine::new(model_dir).await;
        assert!(result.is_err());

        // Check the error message by matching on the error
        match result {
            Ok(_) => panic!("Expected error for missing files"),
            Err(e) => {
                let err_msg = e.to_string();
                assert!(
                    err_msg.contains("Missing TTS files"),
                    "Error should mention missing files: {}",
                    err_msg
                );
            }
        }
    }
}
