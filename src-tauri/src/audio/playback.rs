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

enum StreamMsg {
    Chunk(Vec<f32>, u32),
    Finish,
}

/// Plays audio chunks back to back as they arrive, so speech can start
/// before the whole text is synthesized. The output stream lives on its own
/// thread (rodio's `OutputStream` is not `Send`).
pub struct StreamingPlayer {
    tx: std::sync::mpsc::Sender<StreamMsg>,
    should_stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<Result<()>>>,
}

impl StreamingPlayer {
    /// Start the playback thread. Setting `should_stop` stops playback at once.
    pub fn start(should_stop: Arc<AtomicBool>) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<StreamMsg>();
        let stop = Arc::clone(&should_stop);
        let handle = thread::spawn(move || -> Result<()> {
            let (_stream, stream_handle) = OutputStream::try_default()?;
            let sink = Sink::try_new(&stream_handle)?;
            let mut finished = false;
            loop {
                if stop.load(Ordering::SeqCst) {
                    sink.stop();
                    tracing::info!("Playback stopped");
                    return Ok(());
                }
                match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                    Ok(StreamMsg::Chunk(samples, rate)) => {
                        sink.append(SamplesBuffer::new(1, rate, samples));
                    }
                    Ok(StreamMsg::Finish) => finished = true,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    // The sender is gone without Finish: play what is queued.
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => finished = true,
                }
                if finished && sink.empty() {
                    tracing::info!("Playback completed");
                    return Ok(());
                }
            }
        });
        Self { tx, should_stop, handle: Some(handle) }
    }

    /// Queue a chunk. Returns false once playback has stopped.
    pub fn push(&self, samples: Vec<f32>, sample_rate: u32) -> bool {
        !self.should_stop.load(Ordering::SeqCst)
            && self.tx.send(StreamMsg::Chunk(samples, sample_rate)).is_ok()
    }

    /// Signal that no more chunks are coming and block until playback ends
    /// (all audio played, or stopped).
    pub fn finish_and_wait(mut self) -> Result<()> {
        let _ = self.tx.send(StreamMsg::Finish);
        match self.handle.take().map(|h| h.join()) {
            Some(Ok(result)) => result,
            Some(Err(_)) => Err(anyhow::anyhow!("audio playback thread panicked")),
            None => Ok(()),
        }
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
