pub mod whisper;
pub mod kokoro;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size_bytes: u64,
    pub loaded: bool,
}

/// Trait for Speech-to-Text engines
pub trait SpeechToText: Send + Sync {
    fn transcribe(&self, audio: &[f32]) -> Result<String>;
    fn model_info(&self) -> ModelInfo;
}

/// Audio buffer for TTS output
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
}

/// Trait for Text-to-Speech engines
pub trait TextToSpeech: Send + Sync {
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<AudioBuffer>;
    fn available_voices(&self) -> Vec<VoiceInfo>;
    fn model_info(&self) -> ModelInfo;
}
