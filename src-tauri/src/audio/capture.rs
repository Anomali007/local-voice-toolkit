#![allow(dead_code)]

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
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
    current_level: Arc<AtomicU32>,
    /// Rate of the audio handed to Whisper by `stop()`.
    sample_rate: u32,
    /// Rate the device actually records at (set when the stream starts).
    device_rate: Arc<AtomicU32>,
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
            current_level: Arc::new(AtomicU32::new(0)),
            sample_rate: 16000, // Whisper expects 16kHz
            device_rate: Arc::new(AtomicU32::new(16000)),
            silence_config,
        })
    }

    pub fn start(&self) -> Result<()> {
        if self.is_recording.load(Ordering::SeqCst) {
            return Err(anyhow!("Already recording"));
        }

        // Clear any previous buffer and reset silence trigger
        {
            let mut buf = self.buffer.lock()
                .map_err(|e| anyhow!("Failed to acquire audio buffer lock: {}", e))?;
            buf.clear();
        }
        self.silence_triggered.store(false, Ordering::SeqCst);

        self.is_recording.store(true, Ordering::SeqCst);

        let buffer = Arc::clone(&self.buffer);
        let is_recording = Arc::clone(&self.is_recording);
        let silence_triggered = Arc::clone(&self.silence_triggered);
        let current_level = Arc::clone(&self.current_level);
        let device_rate = Arc::clone(&self.device_rate);
        let silence_config = self.silence_config.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel::<Result<()>>(1);

        // Spawn a dedicated thread for audio capture
        // This keeps the non-Send cpal::Stream contained
        thread::spawn(move || {
            let result = run_capture_loop(
                buffer,
                is_recording,
                silence_triggered,
                current_level,
                device_rate,
                silence_config,
                &started_tx,
            );
            if let Err(e) = result {
                tracing::error!("Audio capture error: {}", e);
                // Reaches start() if the stream never came up.
                let _ = started_tx.try_send(Err(e));
            }
        });

        // Wait for the stream to start, so a failure reaches the caller
        // instead of silently recording nothing.
        let started = started_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap_or_else(|_| Err(anyhow!("Microphone did not start within 3 seconds")));
        if let Err(e) = started {
            self.is_recording.store(false, Ordering::SeqCst);
            return Err(anyhow!("Could not start the microphone: {}", e));
        }

        Ok(())
    }

    /// Check if silence detection triggered an auto-stop.
    pub fn is_silence_triggered(&self) -> bool {
        self.silence_triggered.load(Ordering::SeqCst)
    }

    /// Get the current audio RMS level (0.0 to ~1.0).
    pub fn current_level(&self) -> f32 {
        f32::from_bits(self.current_level.load(Ordering::Relaxed))
    }

    pub fn stop(self) -> Result<Vec<f32>> {
        self.is_recording.store(false, Ordering::SeqCst);

        // Give the capture thread time to finish
        thread::sleep(std::time::Duration::from_millis(100));

        let buffer = self.buffer.lock()
            .map_err(|e| anyhow!("Failed to acquire audio buffer lock: {}", e))?
            .clone();
        let device_rate = self.device_rate.load(Ordering::SeqCst);
        tracing::info!("Captured {} samples at {}Hz", buffer.len(), device_rate);

        Ok(convert_rate(&buffer, device_rate, self.sample_rate))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn run_capture_loop(
    buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    silence_triggered: Arc<AtomicBool>,
    current_level: Arc<AtomicU32>,
    device_rate: Arc<AtomicU32>,
    silence_config: SilenceConfig,
    started: &std::sync::mpsc::SyncSender<Result<()>>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("No input device available"))?;

    // Record in the device's own format. Most Mac microphones only offer
    // 44.1/48 kHz, so asking for Whisper's 16 kHz failed to open the stream
    // and every dictation came back empty. Audio is converted in stop().
    let supported = device
        .default_input_config()
        .map_err(|e| anyhow!("No usable input format: {}", e))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    let channels = config.channels as usize;
    device_rate.store(rate, Ordering::SeqCst);
    tracing::info!(
        "Using input device: {} ({} Hz, {} ch, {:?})",
        device.name().unwrap_or_default(),
        rate,
        channels,
        format
    );

    // Create silence detector if enabled
    let silence_detector = silence_config.enabled.then(|| {
        Mutex::new(SilenceDetector::new(
            silence_config.threshold,
            silence_config.duration_secs,
            rate,
        ))
    });

    // The callback owns these; the loop below watches its own handles.
    let recording = Arc::clone(&is_recording);
    let silenced = Arc::clone(&silence_triggered);

    let on_samples = move |mono: Vec<f32>| {
        // Store audio data, and handle errors gracefully
        match buffer.lock() {
            Ok(mut buf) => buf.extend_from_slice(&mono),
            Err(e) => {
                // Log and continue - don't panic in audio callback
                tracing::error!("Audio buffer lock poisoned, data lost: {}", e);
                return;
            }
        }

        // Compute RMS level for visualization
        let rms = super::silence::calculate_rms(&mono);
        current_level.store(rms.to_bits(), Ordering::Relaxed);

        // Process through silence detector
        if let Some(ref detector_mutex) = silence_detector {
            match detector_mutex.lock() {
                Ok(mut detector) => {
                    if detector.process(&mono) {
                        // Silence duration exceeded - trigger auto-stop
                        silence_triggered.store(true, Ordering::SeqCst);
                        is_recording.store(false, Ordering::SeqCst);
                    }
                }
                Err(e) => tracing::error!("Silence detector lock poisoned: {}", e),
            }
        }
    };

    let err_fn = |err| tracing::error!("Audio stream error: {}", err);
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| on_samples(downmix(data, channels)),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let floats: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                on_samples(downmix(&floats, channels))
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("Unsupported microphone sample format: {:?}", other)),
    };

    stream.play()?;
    let _ = started.try_send(Ok(()));
    tracing::info!(
        "Audio capture started at {}Hz (silence detection: {})",
        rate,
        if silence_config.enabled { "enabled" } else { "disabled" }
    );

    // Keep the stream alive while recording
    while recording.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    // Log reason for stop
    if silenced.load(Ordering::SeqCst) {
        tracing::info!("Audio capture stopped (silence auto-stop)");
    } else {
        tracing::info!("Audio capture stopped (manual)");
    }

    Ok(())
}

/// Average interleaved channels down to mono.
fn downmix(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Convert recorded audio to the rate Whisper expects. Whole-number ratios
/// (48k -> 16k) average each block of samples, which also filters out
/// content above the new Nyquist; other ratios interpolate.
pub fn convert_rate(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return samples.to_vec();
    }
    if from > to && from.is_multiple_of(to) {
        let n = (from / to) as usize;
        return samples
            .chunks(n)
            .map(|block| block.iter().sum::<f32>() / block.len() as f32)
            .collect();
    }
    super::processing::resample(samples, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        assert_eq!(downmix(&[1.0, 3.0, -1.0, 1.0], 2), vec![2.0, 0.0]);
        assert_eq!(downmix(&[0.5, 0.25], 1), vec![0.5, 0.25]);
    }

    #[test]
    fn convert_rate_lengths() {
        let one_sec_48k = vec![0.1; 48_000];
        assert_eq!(convert_rate(&one_sec_48k, 48_000, 16_000).len(), 16_000);
        let one_sec_44k = vec![0.1; 44_100];
        let out = convert_rate(&one_sec_44k, 44_100, 16_000);
        assert!((out.len() as i64 - 16_000).abs() <= 1, "{}", out.len());
        assert_eq!(convert_rate(&[0.3; 10], 16_000, 16_000), vec![0.3; 10]);
    }

    #[test]
    fn decimation_keeps_speech_band_and_drops_above_nyquist() {
        let tone = |f: f32| -> Vec<f32> {
            (0..48_000).map(|i| (2.0 * std::f32::consts::PI * f * i as f32 / 48_000.0).sin()).collect()
        };
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        // 300 Hz (speech) survives nearly intact...
        assert!(rms(&convert_rate(&tone(300.0), 48_000, 16_000)) > 0.65);
        // ...while 15 kHz (above the 8 kHz Nyquist) is strongly attenuated.
        assert!(rms(&convert_rate(&tone(15_000.0), 48_000, 16_000)) < 0.2);
    }
}

#[cfg(test)]
mod device_probe {
    use cpal::traits::{DeviceTrait, HostTrait};

    /// `cargo test --lib probe_default_input -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_default_input() {
        let device = cpal::default_host().default_input_device().expect("no input device");
        println!("device: {}", device.name().unwrap_or_default());
        println!("default: {:?}", device.default_input_config());
        for c in device.supported_input_configs().unwrap() {
            println!("supported: {:?}", c);
        }
    }
}

#[cfg(test)]
mod real_capture {
    use super::*;

    /// `cargo test --lib real_capture -- --ignored --nocapture` (uses the mic)
    #[test]
    #[ignore]
    fn records_one_second_at_whisper_rate() {
        let capture = AudioCapture::with_silence_config(SilenceConfig { enabled: false, ..Default::default() }).unwrap();
        capture.start().expect("stream must start on the default mic");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let device_rate = capture.device_rate.load(Ordering::SeqCst);
        let audio = capture.stop().unwrap();
        let rms = (audio.iter().map(|v| v * v).sum::<f32>() / audio.len().max(1) as f32).sqrt();
        println!("device {} Hz -> {} samples at 16 kHz, rms {:.5}", device_rate, audio.len(), rms);
        assert!((14_000..=18_500).contains(&audio.len()), "{} samples", audio.len());
    }
}
