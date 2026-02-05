use crate::commands::models::{ModelInfo, ModelStatus, ModelType};

pub struct ModelRegistry {
    models: Vec<ModelInfo>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            models: vec![
                // STT Models (Whisper)
                ModelInfo {
                    id: "ggml-tiny.en.bin".to_string(),
                    name: "Whisper Tiny (English)".to_string(),
                    model_type: ModelType::Stt,
                    size_bytes: 39_000_000,
                    size_display: "39 MB".to_string(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin".to_string(),
                    status: ModelStatus::Available,
                    description: "Fastest model, good for quick drafts. ~30x realtime on M1.".to_string(),
                },
                ModelInfo {
                    id: "ggml-base.en.bin".to_string(),
                    name: "Whisper Base (English)".to_string(),
                    model_type: ModelType::Stt,
                    size_bytes: 142_000_000,
                    size_display: "142 MB".to_string(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin".to_string(),
                    status: ModelStatus::Available,
                    description: "Recommended default. Great balance of speed and accuracy. ~15x realtime on M1.".to_string(),
                },
                ModelInfo {
                    id: "ggml-small.en.bin".to_string(),
                    name: "Whisper Small (English)".to_string(),
                    model_type: ModelType::Stt,
                    size_bytes: 488_000_000,
                    size_display: "488 MB".to_string(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin".to_string(),
                    status: ModelStatus::Available,
                    description: "Excellent accuracy for important content. ~6x realtime on M1.".to_string(),
                },
                ModelInfo {
                    id: "ggml-medium.en.bin".to_string(),
                    name: "Whisper Medium (English)".to_string(),
                    model_type: ModelType::Stt,
                    size_bytes: 1_500_000_000,
                    size_display: "1.5 GB".to_string(),
                    download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin".to_string(),
                    status: ModelStatus::Available,
                    description: "Maximum accuracy. ~2x realtime on M1.".to_string(),
                },
                // TTS Models (Kokoro)
                ModelInfo {
                    id: "kokoro-v1.0.onnx".to_string(),
                    name: "Kokoro 82M".to_string(),
                    model_type: ModelType::Tts,
                    size_bytes: 330_000_000,
                    size_display: "330 MB".to_string(),
                    download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/kokoro-v1.0.onnx".to_string(),
                    status: ModelStatus::Available,
                    description: "High-quality TTS with 54 voices. Sub-0.3s generation per sentence.".to_string(),
                },
                ModelInfo {
                    id: "voices-v1.0.bin".to_string(),
                    name: "Kokoro Voice Styles".to_string(),
                    model_type: ModelType::Tts,
                    size_bytes: 5_000_000,
                    size_display: "5 MB".to_string(),
                    download_url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices-v1.0.bin".to_string(),
                    status: ModelStatus::Available,
                    description: "Voice style vectors for Kokoro TTS.".to_string(),
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
