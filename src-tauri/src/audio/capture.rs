use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;

        tracing::info!("Using input device: {}", device.name().unwrap_or_default());

        // Whisper expects 16kHz mono audio
        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(16000),
            buffer_size: cpal::BufferSize::Default,
        };

        Ok(Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: config.sample_rate.0,
        })
    }

    pub fn start(&self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.buffer);

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();
                buf.extend_from_slice(data);
            },
            |err| {
                tracing::error!("Audio capture error: {}", err);
            },
            None,
        )?;

        stream.play()?;

        // Store stream (need interior mutability in real implementation)
        // For now, we'll just keep it alive
        std::mem::forget(stream);

        Ok(())
    }

    pub fn stop(self) -> Result<Vec<f32>> {
        if let Some(stream) = self.stream {
            drop(stream);
        }

        let buffer = Arc::try_unwrap(self.buffer)
            .map_err(|_| anyhow!("Buffer still in use"))?
            .into_inner()
            .map_err(|e| anyhow!("Mutex poisoned: {}", e))?;

        Ok(buffer)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
