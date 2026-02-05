use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;

use super::silence::SilenceDetector;

/// Configuration for silence detection auto-stop.
#[derive(Debug, Clone)]
pub struct SilenceConfig {
    /// Enable silence detection
    pub enabled: bool,
    /// RMS threshold below which audio is considered silent (0.001 to 0.1)
    pub threshold: f32,
    /// Seconds of silence before auto-stop (0.5 to 5.0)
    pub duration_secs: f32,
}

impl Default for SilenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: super::silence::DEFAULT_SILENCE_THRESHOLD,
            duration_secs: super::silence::DEFAULT_SILENCE_DURATION,
        }
    }
}

/// Audio capture handle that can be sent across threads.
/// The actual cpal::Stream runs in a dedicated thread.
pub struct AudioCapture {
    buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    silence_triggered: Arc<AtomicBool>,
    sample_rate: u32,
    silence_config: SilenceConfig,
}

// Implement Send + Sync for AudioCapture
// This is safe because we don't store the cpal::Stream directly,
// instead it runs in a dedicated thread controlled by AtomicBool
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    /// Create a new audio capture with default settings.
    pub fn new() -> Result<Self> {
        Self::with_silence_config(SilenceConfig::default())
    }

    /// Create a new audio capture with custom silence detection settings.
    pub fn with_silence_config(silence_config: SilenceConfig) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No input device available"))?;

        tracing::info!("Using input device: {}", device.name().unwrap_or_default());
        tracing::info!(
            "Silence detection: enabled={}, threshold={:.4}, duration={:.1}s",
            silence_config.enabled,
            silence_config.threshold,
            silence_config.duration_secs
        );

        Ok(Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(AtomicBool::new(false)),
            silence_triggered: Arc::new(AtomicBool::new(false)),
            sample_rate: 16000, // Whisper expects 16kHz
            silence_config,
        })
    }

    pub fn start(&self) -> Result<()> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        // Clear any previous buffer and reset silence trigger
        {
            let mut buf = self.buffer.lock().unwrap();
            buf.clear();
        }
        self.silence_triggered.store(false, Ordering::SeqCst);

        self.is_recording.store(true, Ordering::SeqCst);

        let buffer = Arc::clone(&self.buffer);
        let is_recording = Arc::clone(&self.is_recording);
        let silence_triggered = Arc::clone(&self.silence_triggered);
        let sample_rate = self.sample_rate;
        let silence_config = self.silence_config.clone();

        // Spawn a dedicated thread for audio capture
        // This keeps the non-Send cpal::Stream contained
        thread::spawn(move || {
            let result = run_capture_loop(
                buffer,
                is_recording,
                silence_triggered,
                sample_rate,
                silence_config,
            );
            if let Err(e) = result {
                tracing::error!("Audio capture error: {}", e);
            }
        });

        Ok(())
    }

    /// Check if silence detection triggered an auto-stop.
    pub fn is_silence_triggered(&self) -> bool {
        self.silence_triggered.load(Ordering::SeqCst)
    }

    pub fn stop(self) -> Result<Vec<f32>> {
        self.is_recording.store(false, Ordering::SeqCst);

        // Give the capture thread time to finish
        thread::sleep(std::time::Duration::from_millis(100));

        let buffer = self.buffer.lock().unwrap().clone();
        tracing::info!("Captured {} samples", buffer.len());

        Ok(buffer)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn run_capture_loop(
    buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    silence_triggered: Arc<AtomicBool>,
    sample_rate: u32,
    silence_config: SilenceConfig,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;

    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let buffer_clone = Arc::clone(&buffer);
    let silence_triggered_clone = Arc::clone(&silence_triggered);
    let is_recording_clone = Arc::clone(&is_recording);

    // Create silence detector if enabled
    let silence_detector = if silence_config.enabled {
        Some(Mutex::new(SilenceDetector::new(
            silence_config.threshold,
            silence_config.duration_secs,
            sample_rate,
        )))
    } else {
        None
    };

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Store audio data
            {
                let mut buf = buffer_clone.lock().unwrap();
                buf.extend_from_slice(data);
            }

            // Process through silence detector
            if let Some(ref detector_mutex) = silence_detector {
                let mut detector = detector_mutex.lock().unwrap();
                if detector.process(data) {
                    // Silence duration exceeded - trigger auto-stop
                    silence_triggered_clone.store(true, Ordering::SeqCst);
                    is_recording_clone.store(false, Ordering::SeqCst);
                }
            }
        },
        |err| {
            tracing::error!("Audio stream error: {}", err);
        },
        None,
    )?;

    stream.play()?;
    tracing::info!(
        "Audio capture started at {}Hz (silence detection: {})",
        sample_rate,
        if silence_config.enabled { "enabled" } else { "disabled" }
    );

    // Keep the stream alive while recording
    while is_recording.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Log reason for stop
    if silence_triggered.load(Ordering::SeqCst) {
        tracing::info!("Audio capture stopped (silence auto-stop)");
    } else {
        tracing::info!("Audio capture stopped (manual)");
    }

    Ok(())
}
