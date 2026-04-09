#![allow(dead_code)]

use crate::commands::models::{ModelInfo, ModelStatus, ModelType};
use crate::models::hardware::{HardwareDetector, Tier};

pub struct ModelRegistry {
    models: Vec<ModelInfo>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        let hw = HardwareDetector::detect();
        let chip = &hw.chip_name;
        let tier = &hw.recommended_tier;

        let (tiny_speed, base_speed, small_speed, medium_speed) = match tier {
            Tier::Power => ("~30x realtime", "~15x realtime", "~6x realtime", "~2x realtime"),
            Tier::Standard => ("~25x realtime", "~12x realtime", "~4x realtime", "~1.5x realtime"),
            Tier::Lite => ("~15x realtime", "~8x realtime", "~2x realtime", "may be slow"),
        };

        let (tiny_rec, base_rec, small_rec, medium_rec) = match tier {
            Tier::Power => ("", "⭐ Recommended. ", "🎯 Best value on your hardware. ", &format!("🏆 Your {} handles this easily. ", chip) as &str),
            Tier::Standard => ("", "⭐ Recommended. ", "🎯 Good choice. ", "May use significant RAM. "),
            Tier::Lite => ("⭐ Recommended for your hardware. ", "", "May be slow. ", "Not recommended — too heavy for your hardware. "),
        };

        let coreml_note = if hw.has_neural_engine {
            "Neural Engine ~2x speedup."
        } else {
            "Requires Apple Silicon."
        };

        let tts_speed = match tier {
            Tier::Power => "Sub-0.3s per sentence",
            Tier::Standard => "~0.5s per sentence",
            Tier::Lite => "~1-2s per sentence",
        };

        Self {
            models: vec![
                ModelInfo {
                    id: "ggml-tiny.en.bin".into(),
                    name: "Whisper Tiny (English)".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 39_000_000,
                    size_display: "39 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin".into(),
                    status: ModelStatus::Available,
                    description: format!("⚡ Fastest. {}Quick drafts, chat input. {} on {}. Quality: 6/10.", tiny_rec, tiny_speed, chip),
                },
                ModelInfo {
                    id: "ggml-base.en.bin".into(),
                    name: "Whisper Base (English)".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 142_000_000,
                    size_display: "142 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin".into(),
                    status: ModelStatus::Available,
                    description: format!("{}Best balance for daily use. {} on {}. Quality: 7.5/10.", base_rec, base_speed, chip),
                },
                ModelInfo {
                    id: "ggml-small.en.bin".into(),
                    name: "Whisper Small (English)".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 488_000_000,
                    size_display: "488 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin".into(),
                    status: ModelStatus::Available,
                    description: format!("{}High accuracy for meetings and notes. {} on {}. Quality: 9/10.", small_rec, small_speed, chip),
                },
                ModelInfo {
                    id: "ggml-medium.en.bin".into(),
                    name: "Whisper Medium (English)".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 1_500_000_000,
                    size_display: "1.5 GB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin".into(),
                    status: ModelStatus::Available,
                    description: format!("{}Maximum accuracy for transcription and long-form. {} on {}. Quality: 9.5/10.", medium_rec, medium_speed, chip),
                },
                ModelInfo {
                    id: "ggml-tiny.en-encoder.mlmodelc".into(),
                    name: "CoreML Tiny Encoder".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 26_000_000,
                    size_display: "26 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-encoder.mlmodelc.zip".into(),
                    status: ModelStatus::Available,
                    description: format!("⚡ {} Download alongside Tiny for accelerated performance.", coreml_note),
                },
                ModelInfo {
                    id: "ggml-base.en-encoder.mlmodelc".into(),
                    name: "CoreML Base Encoder".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 38_000_000,
                    size_display: "38 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-encoder.mlmodelc.zip".into(),
                    status: ModelStatus::Available,
                    description: format!("⭐ {} Download with Base for best daily performance.", coreml_note),
                },
                ModelInfo {
                    id: "ggml-small.en-encoder.mlmodelc".into(),
                    name: "CoreML Small Encoder".into(),
                    model_type: ModelType::Stt,
                    size_bytes: 130_000_000,
                    size_display: "130 MB".into(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-encoder.mlmodelc.zip".into(),
                    status: ModelStatus::Available,
                    description: format!("🎯 {} High accuracy + fast with CoreML on {}.", coreml_note, chip),
                },
                ModelInfo {
                    id: "kokoro-v1.0.onnx".into(),
                    name: "Kokoro 82M".into(),
                    model_type: ModelType::Tts,
                    size_bytes: 330_000_000,
                    size_display: "330 MB".into(),
                    download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/kokoro-v1.0.onnx".into(),
                    status: ModelStatus::Available,
                    description: format!("🗣️ 54 voices in 10 languages. {} on {}. Required for TTS.", tts_speed, chip),
                },
                ModelInfo {
                    id: "voices-v1.0.bin".into(),
                    name: "Kokoro Voice Styles".into(),
                    model_type: ModelType::Tts,
                    size_bytes: 5_000_000,
                    size_display: "5 MB".into(),
                    download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices-v1.0.bin".into(),
                    status: ModelStatus::Available,
                    description: "Voice style vectors for all 54 Kokoro voices. Required for TTS.".into(),
                },
            ],
        }
    }

    pub fn get_all_models(&self) -> Vec<ModelInfo> {
        self.models.clone()
    }

    pub fn get_model(&self, id: &str) -> Option<ModelInfo> {
        self.models.iter().find(|m| m.id == id).cloned()
    }

    pub fn get_stt_models(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .filter(|m| m.model_type == ModelType::Stt)
            .cloned()
            .collect()
    }

    pub fn get_tts_models(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .filter(|m| m.model_type == ModelType::Tts)
            .cloned()
            .collect()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Get CoreML encoder models only
impl ModelRegistry {
    pub fn get_coreml_models(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .filter(|m| m.id.ends_with(".mlmodelc"))
            .cloned()
            .collect()
    }

    /// Get the base whisper models (non-CoreML)
    pub fn get_whisper_models(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .filter(|m| m.model_type == ModelType::Stt && m.id.ends_with(".bin"))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_models() {
        let registry = ModelRegistry::new();
        let models = registry.get_all_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_registry_has_coreml_models() {
        let registry = ModelRegistry::new();
        let coreml = registry.get_coreml_models();

        // We should have 3 CoreML models: tiny, base, small
        assert_eq!(coreml.len(), 3);

        // All should be STT type
        for model in &coreml {
            assert_eq!(model.model_type, ModelType::Stt);
        }

        // All should have .mlmodelc suffix
        for model in &coreml {
            assert!(model.id.ends_with(".mlmodelc"));
        }
    }

    #[test]
    fn test_coreml_models_have_zip_urls() {
        let registry = ModelRegistry::new();
        let coreml = registry.get_coreml_models();

        for model in &coreml {
            assert!(
                model.download_url.ends_with(".zip"),
                "CoreML model {} should have .zip download URL",
                model.id
            );
        }
    }

    #[test]
    fn test_each_whisper_model_has_coreml_encoder() {
        let registry = ModelRegistry::new();
        let whisper_models = registry.get_whisper_models();
        let coreml_models = registry.get_coreml_models();

        // tiny, base, small should have CoreML versions
        // (medium doesn't have a CoreML version in our registry)
        let expected_coreml = ["tiny", "base", "small"];

        for expected in expected_coreml {
            let coreml_id = format!("ggml-{}.en-encoder.mlmodelc", expected);
            let found = coreml_models.iter().any(|m| m.id == coreml_id);
            assert!(found, "Expected CoreML model {} not found", coreml_id);

            // Verify corresponding base model exists
            let base_id = format!("ggml-{}.en.bin", expected);
            let base_found = whisper_models.iter().any(|m| m.id == base_id);
            assert!(base_found, "Expected base model {} not found", base_id);
        }
    }

    #[test]
    fn test_get_model_by_id() {
        let registry = ModelRegistry::new();

        // Test getting a base model
        let base = registry.get_model("ggml-base.en.bin");
        assert!(base.is_some());
        assert_eq!(base.unwrap().name, "Whisper Base (English)");

        // Test getting a CoreML model
        let coreml = registry.get_model("ggml-base.en-encoder.mlmodelc");
        assert!(coreml.is_some());
        assert_eq!(coreml.unwrap().name, "CoreML Base Encoder");

        // Test getting a nonexistent model
        let none = registry.get_model("nonexistent-model");
        assert!(none.is_none());
    }

    #[test]
    fn test_stt_models_count() {
        let registry = ModelRegistry::new();
        let stt = registry.get_stt_models();

        // 4 base whisper models + 3 CoreML encoders = 7
        assert_eq!(stt.len(), 7);
    }

    #[test]
    fn test_tts_models_count() {
        let registry = ModelRegistry::new();
        let tts = registry.get_tts_models();

        // kokoro model + voices = 2
        assert_eq!(tts.len(), 2);
    }

    #[test]
    fn test_all_models_have_valid_urls() {
        let registry = ModelRegistry::new();
        let models = registry.get_all_models();

        for model in &models {
            assert!(
                model.download_url.starts_with("https://"),
                "Model {} should have HTTPS URL",
                model.id
            );
            assert!(
                model.download_url.contains("huggingface.co"),
                "Model {} should be hosted on HuggingFace",
                model.id
            );
        }
    }
}
