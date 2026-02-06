#![allow(dead_code)]

use anyhow::Result;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

/// Audio player that can be safely sent across threads.
/// Playback runs in a dedicated thread.
pub struct AudioPlayer {
    is_playing: Arc<AtomicBool>,
    should_stop: Arc<AtomicBool>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            is_playing: Arc::new(AtomicBool::new(false)),
            should_stop: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn play(&self, samples: &[f32], sample_rate: u32) -> Result<()> {
        let samples = samples.to_vec();
        let is_playing = Arc::clone(&self.is_playing);
        let should_stop = Arc::clone(&self.should_stop);

        should_stop.store(false, Ordering::SeqCst);
        is_playing.store(true, Ordering::SeqCst);

        // Spawn playback in a dedicated thread
        thread::spawn(move || {
            if let Err(e) = play_audio_sync(&samples, sample_rate, &should_stop) {
                tracing::error!("Audio playback error: {}", e);
            }
            is_playing.store(false, Ordering::SeqCst);
        });

        Ok(())
    }

    pub fn play_and_wait(&self, samples: &[f32], sample_rate: u32) -> Result<()> {
        let should_stop = Arc::new(AtomicBool::new(false));
        play_audio_sync(samples, sample_rate, &should_stop)
    }

    pub fn stop(&self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::SeqCst)
    }
}

fn play_audio_sync(
    samples: &[f32],
    sample_rate: u32,
    should_stop: &AtomicBool,
) -> Result<()> {
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    let source = SamplesBuffer::new(1, sample_rate, samples.to_vec());
    sink.append(source);

    tracing::info!("Playing {} samples at {}Hz", samples.len(), sample_rate);

    // Wait for playback to complete or stop signal
    while !sink.empty() && !should_stop.load(Ordering::SeqCst) {
        thread::sleep(std::time::Duration::from_millis(10));
    }

    if should_stop.load(Ordering::SeqCst) {
        sink.stop();
        tracing::info!("Playback stopped");
    } else {
        tracing::info!("Playback completed");
    }

    Ok(())
}
