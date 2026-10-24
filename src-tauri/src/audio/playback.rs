#![allow(dead_code)]

use anyhow::Result;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
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

/// Live playback position of a `StreamingPlayer`, readable from any thread.
/// Positions are in *source* seconds: the audio as synthesized, before any
/// speed change, so they stay consistent when the speed changes mid-reading.
#[derive(Default)]
pub struct PlaybackProgress {
    position_ms: AtomicU64,
    chunks_done: AtomicUsize,
}

impl PlaybackProgress {
    /// Source seconds that have actually been played.
    pub fn position_secs(&self) -> f64 {
        self.position_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }

    /// Number of pushed chunks that have finished playing.
    pub fn chunks_done(&self) -> usize {
        self.chunks_done.load(Ordering::Relaxed)
    }
}

/// One piece handed to the sink.
#[derive(Debug, Clone, Copy)]
struct Piece {
    source_secs: f64,
    played_secs: f64,
    ends_chunk: bool,
}

/// Tracks playback position across back-to-back pieces: whole finished
/// pieces plus wall-clock time into the one currently playing, converted
/// back to source time.
#[derive(Default)]
struct PositionTracker {
    pieces: VecDeque<Piece>,
    finished_secs: f64,
    finished_chunks: usize,
    current_started: Option<Instant>,
}

impl PositionTracker {
    fn queued(&mut self, piece: Piece) {
        self.pieces.push_back(piece);
    }

    /// `pending` = pieces the sink still holds. Returns the source position.
    fn update(&mut self, pending: usize, now: Instant) -> f64 {
        while self.pieces.len() > pending {
            if let Some(done) = self.pieces.pop_front() {
                self.finished_secs += done.source_secs;
                self.finished_chunks += done.ends_chunk as usize;
            }
            self.current_started = None;
        }
        match self.pieces.front().copied() {
            Some(front) => {
                let started = *self.current_started.get_or_insert(now);
                let played = now.saturating_duration_since(started).as_secs_f64().min(front.played_secs);
                let ratio = if front.played_secs > 0.0 { front.source_secs / front.played_secs } else { 1.0 };
                self.finished_secs + played * ratio
            }
            None => self.finished_secs,
        }
    }
}

/// Longest piece stretched at once. Speed changes take effect at the next
/// piece, so this bounds how long a change takes to be heard.
const MAX_PIECE_SECS: f64 = 2.5;
const MIN_PIECE_SECS: f64 = 1.2;

/// Where to cut `samples` into a piece: the whole thing when short, else the
/// quietest 10 ms between MIN_ and MAX_PIECE_SECS (a gap between words), so
/// pieces stretched separately join without audible seams.
fn piece_len(samples: &[f32], rate: u32) -> usize {
    let rate = rate.max(1) as f64;
    let max = (MAX_PIECE_SECS * rate) as usize;
    if samples.len() <= max {
        return samples.len();
    }
    let frame = ((rate * 0.010) as usize).max(1);
    let min = (MIN_PIECE_SECS * rate) as usize;
    let mut best = max;
    let mut best_energy = f32::MAX;
    let mut start = min;
    while start + frame <= max {
        let energy: f32 = samples[start..start + frame].iter().map(|x| x * x).sum();
        if energy < best_energy {
            best_energy = energy;
            best = start + frame / 2;
        }
        start += frame;
    }
    best
}

enum StreamMsg {
    Chunk(Vec<f32>, u32),
    Finish,
}

/// Plays audio chunks back to back as they arrive, so speech can start
/// before the whole text is synthesized. Chunks are kept as synthesized and
/// time-stretched just before they play, ~2 s at a time, at whatever speed
/// `speed()` returns then - so a speed change is heard within a couple of
/// seconds instead of after everything already synthesized. The output
/// stream lives on its own thread (rodio's `OutputStream` is not `Send`).
pub struct StreamingPlayer {
    tx: std::sync::mpsc::Sender<StreamMsg>,
    should_stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<Result<()>>>,
    progress: Arc<PlaybackProgress>,
}

impl StreamingPlayer {
    /// Start the playback thread at a fixed speed of 1.0.
    pub fn start(should_stop: Arc<AtomicBool>) -> Self {
        Self::start_with_speed(should_stop, || 1.0)
    }

    /// Start the playback thread. Setting `should_stop` stops playback at
    /// once; `speed` is read before each piece is stretched.
    pub fn start_with_speed(should_stop: Arc<AtomicBool>, speed: fn() -> f32) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<StreamMsg>();
        let stop = Arc::clone(&should_stop);
        let progress = Arc::new(PlaybackProgress::default());
        let shared = Arc::clone(&progress);
        let handle = thread::spawn(move || -> Result<()> {
            let (_stream, stream_handle) = OutputStream::try_default()?;
            let sink = Sink::try_new(&stream_handle)?;
            let mut finished = false;
            let mut tracker = PositionTracker::default();
            let mut pending: VecDeque<(Vec<f32>, u32)> = VecDeque::new();
            loop {
                let pos = tracker.update(sink.len(), Instant::now());
                shared.position_ms.store((pos * 1000.0) as u64, Ordering::Relaxed);
                shared.chunks_done.store(tracker.finished_chunks, Ordering::Relaxed);
                if stop.load(Ordering::SeqCst) {
                    sink.stop();
                    tracing::info!("Playback stopped");
                    return Ok(());
                }
                // Keep about one piece ahead of what is playing.
                while sink.len() < 2 {
                    let Some((samples, rate)) = pending.front_mut() else { break };
                    let rate = *rate;
                    let take = piece_len(samples, rate);
                    let piece: Vec<f32> = samples.drain(..take).collect();
                    let ends_chunk = samples.is_empty();
                    if ends_chunk {
                        pending.pop_front();
                    }
                    let stretched = super::stretch::time_stretch(&piece, rate, speed());
                    tracker.queued(Piece {
                        source_secs: piece.len() as f64 / rate.max(1) as f64,
                        played_secs: stretched.len() as f64 / rate.max(1) as f64,
                        ends_chunk,
                    });
                    sink.append(SamplesBuffer::new(1, rate, stretched));
                }
                match rx.recv_timeout(Duration::from_millis(10)) {
                    Ok(StreamMsg::Chunk(samples, rate)) => {
                        if !samples.is_empty() {
                            pending.push_back((samples, rate));
                        }
                    }
                    Ok(StreamMsg::Finish) => finished = true,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    // The sender is gone without Finish: play what is queued.
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => finished = true,
                }
                if finished && pending.is_empty() && sink.empty() {
                    tracing::info!("Playback completed");
                    return Ok(());
                }
            }
        });
        Self { tx, should_stop, handle: Some(handle), progress }
    }

    /// Shared handle for reading the playback position from another task.
    pub fn progress(&self) -> Arc<PlaybackProgress> {
        Arc::clone(&self.progress)
    }

    /// Queue a chunk (unstretched). Returns false once playback has stopped.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(source: f64, played: f64, ends_chunk: bool) -> Piece {
        Piece { source_secs: source, played_secs: played, ends_chunk }
    }

    #[test]
    fn tracker_reports_source_time_and_finished_chunks() {
        let t0 = Instant::now();
        let mut t = PositionTracker::default();
        // Chunk one in two pieces played at 2x (half the time), then chunk two at 1x.
        t.queued(piece(2.0, 1.0, false));
        t.queued(piece(2.0, 1.0, true));
        t.queued(piece(3.0, 3.0, true));
        assert_eq!(t.update(3, t0), 0.0);
        // 0.5 s of wall time into a 2x piece = 1.0 s of source.
        let p = t.update(3, t0 + Duration::from_millis(500));
        assert!((p - 1.0).abs() < 1e-6, "{p}");
        // First piece done: chunk one is not finished yet.
        let p = t.update(2, t0 + Duration::from_millis(1000));
        assert!((p - 2.0).abs() < 1e-6, "{p}");
        assert_eq!(t.finished_chunks, 0);
        t.update(1, t0 + Duration::from_millis(2000));
        assert_eq!(t.finished_chunks, 1);
        // Time into a piece never exceeds its length.
        let p = t.update(1, t0 + Duration::from_secs(60));
        assert!((p - 7.0).abs() < 1e-6, "{p}");
        t.update(0, t0 + Duration::from_secs(61));
        assert_eq!(t.finished_chunks, 2);
    }

    #[test]
    fn pieces_cut_at_the_quietest_gap() {
        let rate = 24_000u32;
        // 4 s of tone with a 50 ms silence at 2.0 s.
        let mut samples: Vec<f32> = (0..(rate * 4) as usize)
            .map(|i| (i as f32 * 0.05).sin() * 0.5)
            .collect();
        let gap = (2.0 * rate as f64) as usize;
        for s in &mut samples[gap..gap + 1200] {
            *s = 0.0;
        }
        let cut = piece_len(&samples, rate);
        assert!((gap..gap + 1200).contains(&cut), "cut at {cut}, gap at {gap}");
        // Short audio is one piece.
        assert_eq!(piece_len(&samples[..rate as usize], rate), rate as usize);
        // Never longer than the max piece.
        let silent_free: Vec<f32> = vec![0.3; (rate * 10) as usize];
        let cut = piece_len(&silent_free, rate);
        assert!(cut <= (MAX_PIECE_SECS * rate as f64) as usize && cut >= (MIN_PIECE_SECS * rate as f64) as usize);
    }
}
