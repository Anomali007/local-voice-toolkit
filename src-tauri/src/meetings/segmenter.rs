//! Cuts one 16 kHz mono stream (mic or system audio) into utterances for
//! transcription.
//!
//! 30 ms frames are classed as speech when their RMS is `margin_db` above the
//! noise floor (the quietest frame of the last `floor_window_ms`) and above
//! `min_speech_dbfs`. A segment closes after
//! `min_silence_ms` of silence once it holds `min_speech_ms` of speech, or
//! after `max_silence_ms` of silence whatever its length (so a lone "yeah"
//! still lands). Past `max_segment_ms` it is force-cut at the quietest
//! `quiet_window_ms` of the last `search_window_ms`. Segments get
//! `pad_ms` of padding on each side and never overlap.

/// Sample rate the segmenter works at (same as the STT engines).
pub const RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct SegmenterConfig {
    pub frame_ms: u32,
    pub margin_db: f32,
    pub floor_window_ms: u32,
    pub min_speech_dbfs: f32,
    pub min_silence_ms: u32,
    pub min_speech_ms: u32,
    pub max_silence_ms: u32,
    /// Segments with less speech than this are dropped (clicks, coughs).
    pub min_keep_ms: u32,
    pub max_segment_ms: u32,
    pub search_window_ms: u32,
    pub quiet_window_ms: u32,
    pub pad_ms: u32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            frame_ms: 30,
            margin_db: 9.0,
            floor_window_ms: 3_000,
            min_speech_dbfs: -60.0,
            min_silence_ms: 600,
            min_speech_ms: 2_000,
            max_silence_ms: 1_500,
            min_keep_ms: 250,
            max_segment_ms: 15_000,
            search_window_ms: 2_000,
            quiet_window_ms: 100,
            pad_ms: 200,
        }
    }
}

/// One utterance. `start`/`end` are sample offsets in the stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub samples: Vec<f32>,
    /// Cut by the `max_segment_ms` limit rather than at a pause.
    pub forced: bool,
}

impl Segment {
    pub fn start_secs(&self) -> f64 {
        self.start as f64 / RATE as f64
    }
    pub fn duration_secs(&self) -> f64 {
        (self.end - self.start) as f64 / RATE as f64
    }
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    start: u64,
    db: f32,
    speech: bool,
}

#[derive(Debug)]
struct Open {
    start: u64,
    last_speech_end: u64,
}

pub struct Segmenter {
    cfg: SegmenterConfig,
    frame_len: usize,
    /// Audio from `buf_start` up to `now`.
    buf: Vec<f32>,
    buf_start: u64,
    /// Samples not yet making up a whole frame.
    partial: Vec<f32>,
    now: u64,
    /// No segment may start before this (the end of the previous one).
    min_start: u64,
    /// Recent frame levels, for the minimum-statistics noise floor.
    recent_db: std::collections::VecDeque<f32>,
    open: Option<Open>,
    /// Frames since the open segment's start (empty when none is open).
    frames: Vec<Frame>,
}

fn ms_to_samples(ms: u32) -> u64 {
    ms as u64 * RATE as u64 / 1000
}

fn frame_db(frame: &[f32]) -> f32 {
    let db = crate::audio::silence::rms_to_db(crate::audio::silence::calculate_rms(frame));
    db.max(-100.0)
}

impl Segmenter {
    pub fn new(cfg: SegmenterConfig) -> Self {
        let frame_len = ms_to_samples(cfg.frame_ms) as usize;
        Self {
            cfg,
            frame_len,
            buf: Vec::new(),
            buf_start: 0,
            partial: Vec::new(),
            now: 0,
            min_start: 0,
            recent_db: std::collections::VecDeque::new(),
            open: None,
            frames: Vec::new(),
        }
    }

    /// Feeds 16 kHz mono samples; returns the segments they completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Segment> {
        let mut out = Vec::new();
        self.partial.extend_from_slice(samples);
        let whole = self.partial.len() / self.frame_len * self.frame_len;
        let ready: Vec<f32> = self.partial.drain(..whole).collect();
        for frame in ready.chunks(self.frame_len) {
            self.process_frame(frame, &mut out);
        }
        out
    }

    /// Ends the stream, returning the segment still open, if it has speech.
    pub fn flush(&mut self) -> Option<Segment> {
        let partial = std::mem::take(&mut self.partial);
        let mut out = Vec::new();
        if !partial.is_empty() {
            let mut frame = partial;
            frame.resize(self.frame_len, 0.0);
            self.process_frame(&frame, &mut out);
        }
        let open = self.open.take()?;
        let speech = self.speech_ms_since(open.start);
        let end = (open.last_speech_end + ms_to_samples(self.cfg.pad_ms)).min(self.now);
        let seg = (speech >= self.cfg.min_keep_ms).then(|| self.take(open.start, end, false));
        out.into_iter().chain(seg).last()
    }

    fn process_frame(&mut self, frame: &[f32], out: &mut Vec<Segment>) {
        let start = self.now;
        let db = frame_db(frame);
        // Speech has gaps between words; steady noise doesn't. So the
        // quietest recent frame tracks the noise, and a fan that switches on
        // stops counting as speech after one window.
        let window = (self.cfg.floor_window_ms / self.cfg.frame_ms).max(1) as usize;
        if self.recent_db.len() == window {
            self.recent_db.pop_front();
        }
        self.recent_db.push_back(db);
        let floor = self.recent_db.iter().copied().fold(f32::INFINITY, f32::min);
        let speech = db >= self.cfg.min_speech_dbfs && db >= floor + self.cfg.margin_db;

        self.buf.extend_from_slice(frame);
        self.now += frame.len() as u64;
        let end = self.now;

        if self.open.is_none() {
            if speech {
                let seg_start = start
                    .saturating_sub(ms_to_samples(self.cfg.pad_ms))
                    .max(self.min_start)
                    .max(self.buf_start);
                self.open = Some(Open {
                    start: seg_start,
                    last_speech_end: end,
                });
                self.frames.clear();
                self.frames.push(Frame { start, db, speech });
            } else {
                // Keep only the pre-roll padding.
                let keep = ms_to_samples(self.cfg.pad_ms);
                let new_start = end.saturating_sub(keep).max(self.buf_start);
                self.drain_to(new_start);
            }
            return;
        }

        self.frames.push(Frame { start, db, speech });
        let open = self.open.as_mut().unwrap();
        if speech {
            open.last_speech_end = end;
        }
        let seg_start = open.start;
        let last_speech_end = open.last_speech_end;
        let silence = end - last_speech_end;
        let speech_ms = self.speech_ms_since(seg_start);
        let pad = ms_to_samples(self.cfg.pad_ms);

        if silence >= ms_to_samples(self.cfg.min_silence_ms) && speech_ms >= self.cfg.min_speech_ms
            || silence >= ms_to_samples(self.cfg.max_silence_ms)
        {
            self.open = None;
            let cut = (last_speech_end + pad).min(end);
            if speech_ms >= self.cfg.min_keep_ms {
                out.push(self.take(seg_start, cut, false));
            } else {
                self.drain_to(cut);
                self.min_start = cut;
            }
            self.frames.clear();
        } else if end - seg_start >= ms_to_samples(self.cfg.max_segment_ms) {
            let cut = self.quietest_point(end);
            out.push(self.take(seg_start, cut, true));
            self.frames.retain(|f| f.start >= cut);
            let last = self.frames.iter().rev().find(|f| f.speech);
            self.open = Some(Open {
                start: cut,
                last_speech_end: last.map_or(cut, |f| f.start + self.frame_len as u64),
            });
        }
    }

    /// Centre of the quietest `quiet_window_ms` within the last
    /// `search_window_ms`, on a frame boundary.
    fn quietest_point(&self, end: u64) -> u64 {
        let from = end.saturating_sub(ms_to_samples(self.cfg.search_window_ms));
        let win = (self.cfg.quiet_window_ms.div_ceil(self.cfg.frame_ms)).max(1) as usize;
        let frames: Vec<&Frame> = self.frames.iter().filter(|f| f.start >= from).collect();
        if frames.len() < win {
            return end;
        }
        let mut best = (f32::INFINITY, end);
        for w in frames.windows(win) {
            // Mean power over the window, in dB, so one loud frame counts.
            let p: f32 = w.iter().map(|f| 10f32.powf(f.db / 10.0)).sum::<f32>() / win as f32;
            if p < best.0 {
                best = (p, w[win / 2].start);
            }
        }
        best.1
    }

    fn speech_ms_since(&self, start: u64) -> u32 {
        let n = self
            .frames
            .iter()
            .filter(|f| f.speech && f.start >= start)
            .count();
        n as u32 * self.cfg.frame_ms
    }

    /// Removes and returns [start, end) of the buffer as a segment.
    fn take(&mut self, start: u64, end: u64, forced: bool) -> Segment {
        let a = (start - self.buf_start) as usize;
        let b = (end - self.buf_start) as usize;
        let samples = self.buf[a..b].to_vec();
        self.drain_to(end);
        self.min_start = end;
        Segment {
            start,
            end,
            samples,
            forced,
        }
    }

    fn drain_to(&mut self, to: u64) {
        if to > self.buf_start {
            let n = ((to - self.buf_start) as usize).min(self.buf.len());
            self.buf.drain(..n);
            self.buf_start += n as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: usize = RATE as usize;

    /// Speech-like burst: a 220 Hz tone with a 4 Hz syllable envelope.
    fn burst(secs: f32, amp: f32) -> Vec<f32> {
        let n = (secs * R as f32) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / R as f32;
                let env = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                amp * env * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    fn zeros(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * R as f32) as usize]
    }

    /// Deterministic noise at roughly `dbfs` RMS.
    fn noise(secs: f32, dbfs: f32) -> Vec<f32> {
        let amp = 10f32.powf(dbfs / 20.0) * 3f32.sqrt();
        let mut x: u32 = 12345;
        (0..(secs * R as f32) as usize)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                amp * ((x as f32 / u32::MAX as f32) * 2.0 - 1.0)
            })
            .collect()
    }

    fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
        a.iter().zip(b).map(|(x, y)| x + y).collect()
    }

    fn run(audio: &[f32], chunk: usize) -> Vec<Segment> {
        let mut s = Segmenter::new(SegmenterConfig::default());
        let mut out: Vec<Segment> = audio.chunks(chunk).flat_map(|c| s.push(c)).collect();
        out.extend(s.flush());
        out
    }

    fn secs(x: u64) -> f64 {
        x as f64 / R as f64
    }

    #[test]
    fn silence_and_noise_alone_make_no_segments() {
        assert!(run(&zeros(10.0), 1600).is_empty());
        assert!(run(&noise(10.0, -55.0), 1600).is_empty());
    }

    #[test]
    fn one_utterance_is_padded_on_both_sides() {
        let audio = [zeros(1.0), burst(3.0, 0.3), zeros(2.0)].concat();
        let segs = run(&audio, 1600);
        assert_eq!(segs.len(), 1);
        let s = &segs[0];
        assert!(
            (secs(s.start) - 0.8).abs() < 0.05,
            "start {}",
            secs(s.start)
        );
        assert!((secs(s.end) - 4.2).abs() < 0.05, "end {}", secs(s.end));
        assert_eq!(s.samples.len() as u64, s.end - s.start);
        assert!(!s.forced);
    }

    #[test]
    fn short_gap_does_not_split() {
        let audio = [
            zeros(0.5),
            burst(1.5, 0.3),
            zeros(0.3),
            burst(1.5, 0.3),
            zeros(2.0),
        ]
        .concat();
        assert_eq!(run(&audio, 1600).len(), 1);
    }

    #[test]
    fn pause_splits_once_enough_speech() {
        let audio = [
            zeros(0.5),
            burst(2.5, 0.3),
            zeros(0.8),
            burst(2.5, 0.3),
            zeros(2.0),
        ]
        .concat();
        let segs = run(&audio, 1600);
        assert_eq!(segs.len(), 2);
        assert!(segs[0].end <= segs[1].start, "overlap");
    }

    #[test]
    fn short_reply_lands_after_max_silence() {
        let audio = [zeros(0.5), burst(0.5, 0.3), zeros(3.0)].concat();
        let mut s = Segmenter::new(SegmenterConfig::default());
        let segs = s.push(&audio);
        assert_eq!(segs.len(), 1, "emitted before flush");
    }

    #[test]
    fn click_is_dropped() {
        let audio = [zeros(0.5), burst(0.06, 0.5), zeros(3.0)].concat();
        assert!(run(&audio, 1600).is_empty());
    }

    #[test]
    fn monologue_is_force_cut_at_dips_without_overlap() {
        // 40 s of speech with a 150 ms dip every 1.5 s: too short to end a
        // segment, but where a forced cut should land (every 2 s search
        // window holds one).
        let mut audio = zeros(0.5);
        for _ in 0..27 {
            audio.extend(burst(1.35, 0.3));
            audio.extend(zeros(0.15));
        }
        audio.extend(zeros(2.0));
        let segs = run(&audio, 1600);
        assert!(segs.len() >= 3, "{} segments", segs.len());
        let mut covered = 0u64;
        for w in segs.windows(2) {
            assert_eq!(w[0].end, w[1].start, "forced cuts should be contiguous");
        }
        for s in &segs {
            assert!(s.duration_secs() <= 15.0 + 1e-6, "{}", s.duration_secs());
            covered += s.end - s.start;
            if s.forced {
                // Cut inside a dip: dips span 1.35..1.5 of each 1.5 s cycle.
                let t = secs(s.end) - 0.5;
                let into_cycle = t.rem_euclid(1.5);
                assert!(into_cycle >= 1.33 || into_cycle <= 0.02, "cut at {t}");
            }
        }
        assert!(secs(covered) > 39.0);
    }

    #[test]
    fn chunk_size_does_not_change_segments() {
        let audio = [
            zeros(0.7),
            burst(2.4, 0.3),
            zeros(0.9),
            burst(0.4, 0.2),
            zeros(2.0),
            burst(17.0, 0.3),
            zeros(1.0),
        ]
        .concat();
        let a = run(&audio, 1600);
        assert_eq!(a, run(&audio, 333));
        assert_eq!(a, run(&audio, 48_000));
    }

    #[test]
    fn speech_over_mic_noise_floor() {
        let floor = noise(12.0, -58.0);
        let speech = [
            zeros(2.0),
            burst(3.0, 0.05),
            zeros(2.0),
            burst(3.0, 0.05),
            zeros(2.0),
        ]
        .concat();
        let segs = run(&mix(&floor, &speech), 1600);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn floor_adapts_to_new_steady_noise() {
        // Fan switches on at -40 dBFS: after a forced cut or two it must stop
        // reading as speech.
        let audio = [zeros(1.0), noise(60.0, -40.0)].concat();
        let segs = run(&audio, 1600);
        let total: f64 = segs.iter().map(|s| s.duration_secs()).sum();
        assert!(total < 20.0, "{total:.1}s of noise segmented");
    }
}

/// Opt-in S2 benchmark: segments each WAV as a live stream (100 ms chunks),
/// transcribes every segment with Parakeet, and prints numbers only (never
/// transcript text):
/// `SEG_BENCH_WAVS=a.wav,b.wav cargo test --release --lib segmenter_real -- --ignored --nocapture`
/// With `<wav>.turns.json` (oracle `[start, end]` sample ranges) it also
/// transcribes the oracle turns, so the WER between the two isolates what
/// segmentation costs; with `<wav>.txt` it scores both against the reference.
#[cfg(test)]
mod real_audio {
    use super::*;
    use crate::engines::stt;
    use std::path::Path;
    use std::time::Instant;

    fn words(s: &str) -> Vec<String> {
        s.to_lowercase()
            .replace(['-', '—'], " ")
            .split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '\'')
                    .collect::<String>()
            })
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// Word edit distance, plus the hypothesis positions of each error.
    fn align(reference: &[String], hyp: &[String]) -> (usize, Vec<usize>) {
        let (n, m) = (reference.len(), hyp.len());
        let mut d = vec![vec![0usize; m + 1]; n + 1];
        for (i, row) in d.iter_mut().enumerate() {
            row[0] = i;
        }
        for (j, cell) in d[0].iter_mut().enumerate() {
            *cell = j;
        }
        for i in 1..=n {
            for j in 1..=m {
                let sub = d[i - 1][j - 1] + usize::from(reference[i - 1] != hyp[j - 1]);
                d[i][j] = sub.min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
            }
        }
        let (mut i, mut j, mut at) = (n, m, Vec::new());
        while i > 0 || j > 0 {
            if i > 0
                && j > 0
                && d[i][j] == d[i - 1][j - 1] + usize::from(reference[i - 1] != hyp[j - 1])
            {
                if reference[i - 1] != hyp[j - 1] {
                    at.push(j - 1);
                }
                i -= 1;
                j -= 1;
            } else if i > 0 && d[i][j] == d[i - 1][j] + 1 {
                at.push(j.saturating_sub(1));
                i -= 1;
            } else {
                at.push(j - 1);
                j -= 1;
            }
        }
        (d[n][m], at)
    }

    fn pct(a: usize, b: usize) -> f64 {
        100.0 * a as f64 / b.max(1) as f64
    }

    fn percentile(v: &mut [f64], p: f64) -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f64 * p).round() as usize]
    }

    #[test]
    #[ignore]
    fn segmenter_real() {
        let wavs = std::env::var("SEG_BENCH_WAVS").expect("set SEG_BENCH_WAVS=a.wav,b.wav");
        let model = dirs::data_dir()
            .unwrap()
            .join("com.blahcubed.app/models/stt")
            .join(crate::engines::parakeet::MODEL_ID);
        assert!(stt::is_available(&model), "Parakeet not downloaded");
        let model = model.to_string_lossy().to_string();
        let t0 = Instant::now();
        stt::preload(&model).unwrap();
        println!("parakeet load {:.1}s", t0.elapsed().as_secs_f64());

        for wav in wavs.split(',') {
            let (raw, rate) = crate::audio::processing::read_wav(Path::new(wav)).unwrap();
            let audio = crate::audio::capture::convert_rate(&raw, rate, RATE);
            let audio_secs = audio.len() as f64 / RATE as f64;

            // Live: 100 ms chunks, noting when each segment is emitted.
            let chunk = RATE as usize / 10;
            let mut seg = Segmenter::new(SegmenterConfig::default());
            let mut emitted: Vec<(Segment, u64)> = Vec::new();
            let seg_t0 = Instant::now();
            for (k, c) in audio.chunks(chunk).enumerate() {
                let now = ((k + 1) * chunk).min(audio.len()) as u64;
                emitted.extend(seg.push(c).into_iter().map(|s| (s, now)));
            }
            emitted.extend(seg.flush().map(|s| (s, audio.len() as u64)));
            let seg_cpu = seg_t0.elapsed().as_secs_f64();

            let mut hyp = String::new();
            let mut boundaries = Vec::new();
            let mut lag = Vec::new();
            let mut stt_secs = 0.0;
            let pad = ms_to_samples(SegmenterConfig::default().pad_ms);
            for (s, at) in &emitted {
                let t = Instant::now();
                let text = stt::transcribe(&model, &s.samples).unwrap();
                let dt = t.elapsed().as_secs_f64();
                stt_secs += dt;
                // Speech end → line on screen: waiting for the pause, then STT.
                lag.push((*at - (s.end - pad).min(*at)) as f64 / RATE as f64 + dt);
                boundaries.push(words(&hyp).len());
                hyp.push(' ');
                hyp.push_str(&text);
            }
            let hyp_w = words(&hyp);
            let durs: Vec<f64> = emitted.iter().map(|(s, _)| s.duration_secs()).collect();
            let forced = emitted.iter().filter(|(s, _)| s.forced).count();
            println!(
                "{wav}\n  audio {audio_secs:.1}s | segments {} (forced {forced}) | dur mean {:.1}s max {:.1}s | segmenter {:.1} ms total",
                emitted.len(),
                durs.iter().sum::<f64>() / durs.len().max(1) as f64,
                durs.iter().cloned().fold(0.0, f64::max),
                seg_cpu * 1000.0
            );
            println!(
                "  stt {stt_secs:.2}s wall for {audio_secs:.0}s audio (RTF {:.3}) | speech end → line p50 {:.2}s p95 {:.2}s max {:.2}s",
                stt_secs / audio_secs,
                percentile(&mut lag.clone(), 0.5),
                percentile(&mut lag.clone(), 0.95),
                lag.iter().cloned().fold(0.0, f64::max)
            );

            let turns_path = format!("{wav}.turns.json");
            let mut oracle_w = None;
            if let Ok(j) = std::fs::read_to_string(&turns_path) {
                let turns: Vec<[u64; 2]> = serde_json::from_str(&j).unwrap();
                let scale = RATE as f64 / rate as f64;
                let mut text = String::new();
                for [a, b] in turns {
                    let a = ((a as f64 * scale) as usize).saturating_sub(pad as usize);
                    let b = (((b as f64 * scale) as usize) + pad as usize).min(audio.len());
                    text.push(' ');
                    text.push_str(&stt::transcribe(&model, &audio[a..b]).unwrap());
                }
                oracle_w = Some(words(&text));
            }
            if let Some(o) = &oracle_w {
                let (e, at) = align(o, &hyp_w);
                // Errors within one word of a segment boundary = cut damage.
                let near = at
                    .iter()
                    .filter(|&&j| boundaries.iter().any(|&b| j + 1 >= b && j <= b + 1))
                    .count();
                println!(
                    "  segmenter vs oracle turns: {e} word diffs / {} words ({:.1}%), {near} within 1 word of a cut",
                    o.len(),
                    pct(e, o.len())
                );
            }
            if let Ok(r) = std::fs::read_to_string(format!("{wav}.txt")) {
                let r = words(&r);
                let (e_seg, _) = align(&r, &hyp_w);
                print!("  WER vs reference: segmenter {:.1}%", pct(e_seg, r.len()));
                if let Some(o) = &oracle_w {
                    let (e_or, _) = align(&r, o);
                    print!(" | oracle turns {:.1}%", pct(e_or, r.len()));
                }
                println!(" ({} ref words)", r.len());
            }
        }
    }
}
