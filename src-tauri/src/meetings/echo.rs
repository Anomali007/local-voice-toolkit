//! Phase 0 spike S4: how much of the far side ("Them", from the system-audio
//! tap) comes back through the microphone ("Me") when the call plays on
//! speakers. Two measures:
//! - level: in 10 ms frames where Them is talking, how far the mic sits above
//!   its own noise floor, and how well the two level curves track each other
//!   at the best delay (`echo_stats`);
//! - text: Me segments whose words mostly appear in what Them said around the
//!   same time (`echo_overlap`), i.e. lines the transcript would duplicate.

use super::segmenter::RATE;

const FRAME: usize = RATE as usize / 100; // 10 ms
const MAX_LAG_FRAMES: usize = 50; // 500 ms
const FLOOR_DB: f32 = -100.0;

#[derive(Debug, Clone, Copy)]
pub struct EchoStats {
    /// Delay of the mic behind the tap at the best match, in ms.
    pub lag_ms: u32,
    /// Correlation of the two level curves at that delay, over the frames
    /// where Them is talking (-1..1). Echo makes the mic rise and fall with
    /// Them; without it the mic sits at its floor and this stays near 0.
    pub level_corr: f32,
    /// Seconds in which Them is talking.
    pub them_active_secs: f32,
    /// Mic level above its noise floor during those frames, in dB.
    pub leak_db_median: f32,
    pub leak_db_p90: f32,
}

fn frame_dbs(x: &[f32]) -> Vec<f32> {
    x.chunks(FRAME)
        .map(|c| {
            let ms = c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32;
            if ms > 0.0 {
                (10.0 * ms.log10()).max(FLOOR_DB)
            } else {
                FLOOR_DB
            }
        })
        .collect()
}

fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f32 * p).round() as usize]
}

fn corr(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f32>() / n as f32;
    let mb = b.iter().sum::<f32>() / n as f32;
    let (mut sab, mut saa, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        let (dx, dy) = ((x - ma) as f64, (y - mb) as f64);
        sab += dx * dy;
        saa += dx * dx;
        sbb += dy * dy;
    }
    if saa == 0.0 || sbb == 0.0 {
        0.0
    } else {
        (sab / (saa * sbb).sqrt()) as f32
    }
}

/// Both inputs are 16 kHz mono on the same timeline.
pub fn echo_stats(me: &[f32], them: &[f32]) -> EchoStats {
    let me_db = frame_dbs(me);
    let them_db = frame_dbs(them);
    let n = me_db.len().min(them_db.len());

    // Them is talking when it is well above its own floor (the tap delivers
    // exact zeros between turns, so the floor is usually -100).
    let them_floor = percentile(&them_db[..n], 0.1);
    let active_at = (them_floor + 9.0).max(-60.0);
    let me_floor = percentile(&me_db[..n], 0.1);

    // Only frames where Them is talking: over the whole call, Me talking in
    // Them's gaps would drag the correlation negative and hide the echo.
    let mut best = (0usize, f32::MIN);
    for lag in 0..=MAX_LAG_FRAMES.min(n.saturating_sub(2)) {
        let (a, b): (Vec<f32>, Vec<f32>) = (0..n - lag)
            .filter(|&i| them_db[i] > active_at)
            .map(|i| (them_db[i], me_db[i + lag]))
            .unzip();
        let c = corr(&a, &b);
        if c > best.1 {
            best = (lag, c);
        }
    }
    let lag = best.0;
    let leak: Vec<f32> = (0..n - lag)
        .filter(|&i| them_db[i] > active_at)
        .map(|i| me_db[i + lag] - me_floor)
        .collect();
    EchoStats {
        lag_ms: (lag * 10) as u32,
        level_corr: best.1.max(-1.0),
        them_active_secs: leak.len() as f32 / 100.0,
        leak_db_median: percentile(&leak, 0.5),
        leak_db_p90: percentile(&leak, 0.9),
    }
}

/// Lower-cased words with punctuation dropped.
pub fn words(s: &str) -> Vec<String> {
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

/// Share of `me` words that also occur in `them` (0..1).
pub fn echo_overlap(me: &[String], them: &[String]) -> f32 {
    if me.is_empty() {
        return 0.0;
    }
    let them: std::collections::HashSet<&str> = them.iter().map(|w| w.as_str()).collect();
    me.iter().filter(|w| them.contains(w.as_str())).count() as f32 / me.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise in -1..1.
    fn noise(n: usize, seed: u64) -> Vec<f32> {
        let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (0..n)
            .map(|_| {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((s >> 33) as f32 / (1u64 << 31) as f32) - 1.0
            })
            .collect()
    }

    /// Talk bursts: on for `on` s, off for a varying 0.4–1.6 s, around
    /// -20 dBFS with a 4 Hz syllable rhythm (speech level is never flat).
    fn bursts(secs: usize, on: f32, seed: u64, phase: bool) -> Vec<f32> {
        let n = secs * RATE as usize;
        let src = noise(n, seed);
        let mut out = vec![0f32; n];
        let (mut t, mut k, mut talking) = (0usize, 0usize, phase);
        while t < n {
            let len = if talking {
                (on * RATE as f32) as usize
            } else {
                ((0.4 + (k % 7) as f32 * 0.2) * RATE as f32) as usize
            };
            if talking {
                for i in t..(t + len).min(n) {
                    let syl = (std::f32::consts::TAU * 4.0 * i as f32 / RATE as f32)
                        .sin()
                        .abs();
                    out[i] = src[i] * 0.17 * (0.1 + 0.9 * syl);
                }
            }
            t += len;
            k += 1;
            talking = !talking;
        }
        out
    }

    fn room(n: usize) -> Vec<f32> {
        noise(n, 99).iter().map(|s| s * 0.002).collect() // about -58 dBFS
    }

    #[test]
    fn speakers_leak_is_found_at_its_delay() {
        let them = bursts(60, 1.2, 1, true);
        let mut me = room(them.len());
        let delay = RATE as usize * 80 / 1000;
        for i in delay..me.len() {
            me[i] += them[i - delay] * 0.1; // -20 dB acoustic path
        }
        let s = echo_stats(&me, &them);
        assert!((70..=90).contains(&s.lag_ms), "{s:?}");
        assert!(s.level_corr > 0.5, "{s:?}");
        assert!(s.leak_db_median > 15.0, "{s:?}");
        assert!(s.them_active_secs > 20.0, "{s:?}");
    }

    #[test]
    fn headphones_turn_taking_shows_no_leak() {
        // Me talks only in Them's gaps (opposite phase), no acoustic path.
        let them = bursts(60, 1.2, 1, true);
        // Me answers in some of Them's gaps, not all of them.
        let gaps: Vec<f32> = them
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if *s == 0.0 && (i / (3 * RATE as usize)).is_multiple_of(2) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let mine = noise(them.len(), 7);
        let r = room(them.len());
        let me: Vec<f32> = (0..them.len())
            .map(|i| r[i] + mine[i] * 0.17 * gaps[i])
            .collect();
        let s = echo_stats(&me, &them);
        assert!(s.leak_db_median < 3.0, "{s:?}");
        assert!(s.level_corr.abs() < 0.2, "{s:?}");
    }

    #[test]
    fn echo_shows_through_my_own_talking() {
        // Speakers AND turn-taking: Me talks in Them's gaps, and Them leaks
        // back at -20 dB. Whole-call correlation goes negative here.
        let them = bursts(60, 1.2, 1, true);
        // Me answers in some of Them's gaps, not all of them.
        let gaps: Vec<f32> = them
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if *s == 0.0 && (i / (3 * RATE as usize)).is_multiple_of(2) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        let mine = noise(them.len(), 7);
        let r = room(them.len());
        let delay = RATE as usize / 10;
        let me: Vec<f32> = (0..them.len())
            .map(|i| {
                let echo = if i >= delay {
                    them[i - delay] * 0.1
                } else {
                    0.0
                };
                r[i] + mine[i] * 0.17 * gaps[i] + echo
            })
            .collect();
        let s = echo_stats(&me, &them);
        assert!((90..=110).contains(&s.lag_ms), "{s:?}");
        assert!(s.level_corr > 0.5, "{s:?}");
    }

    #[test]
    fn silent_far_side_measures_nothing() {
        let them = vec![0f32; 10 * RATE as usize];
        let s = echo_stats(&room(them.len()), &them);
        assert_eq!(s.them_active_secs, 0.0);
        assert_eq!(s.leak_db_median, 0.0);
    }

    #[test]
    fn overlap_counts_shared_words() {
        let them = words("So what does ongoing support cost?");
        assert_eq!(
            echo_overlap(&words("What does ongoing support cost"), &them),
            1.0
        );
        assert_eq!(echo_overlap(&words("Five hundred a month"), &them), 0.0);
        assert_eq!(echo_overlap(&[], &them), 0.0);
        assert!((echo_overlap(&words("support costs five"), &them) - 1.0 / 3.0).abs() < 1e-6);
    }
}

/// Opt-in: `S4_DIR=<dir with me.wav + them.wav> cargo test --lib echo_real
/// -- --ignored --nocapture`. Prints numbers only, never transcript text.
#[cfg(test)]
mod real_audio {
    use super::super::segmenter::{Segment, Segmenter, SegmenterConfig};
    use super::*;
    use crate::engines::stt;
    use std::path::Path;

    fn segments(x: &[f32]) -> Vec<Segment> {
        let mut seg = Segmenter::new(SegmenterConfig::default());
        let mut out = Vec::new();
        for c in x.chunks(RATE as usize / 10) {
            out.extend(seg.push(c));
        }
        out.extend(seg.flush());
        out
    }

    #[test]
    #[ignore]
    fn echo_real() {
        let dirs = std::env::var("S4_DIR").expect("set S4_DIR=<dir>[,<dir>]");
        let model = dirs::data_dir()
            .unwrap()
            .join("com.blahcubed.app/models/stt")
            .join(crate::engines::parakeet::MODEL_ID);
        assert!(stt::is_available(&model), "Parakeet not downloaded");
        let model = model.to_string_lossy().to_string();
        stt::preload(&model).unwrap();

        for dir in dirs.split(',') {
            let read = |name: &str| {
                let (x, rate) =
                    crate::audio::processing::read_wav(&Path::new(dir).join(name)).unwrap();
                crate::audio::capture::convert_rate(&x, rate, RATE)
            };
            let (me, them) = (read("me.wav"), read("them.wav"));
            let s = echo_stats(&me, &them);
            println!(
                "{dir}\n  {:.0}s | them talking {:.0}s | mic above floor while them talks: median {:.1} dB, p90 {:.1} dB | level corr {:.2} at {} ms",
                me.len() as f32 / RATE as f32,
                s.them_active_secs,
                s.leak_db_median,
                s.leak_db_p90,
                s.level_corr,
                s.lag_ms
            );

            let text = |segs: &[Segment]| -> Vec<(u64, u64, Vec<String>)> {
                segs.iter()
                    .map(|g| {
                        (
                            g.start,
                            g.end,
                            words(&stt::transcribe(&model, &g.samples).unwrap()),
                        )
                    })
                    .collect()
            };
            let me_t = text(&segments(&me));
            let them_t = text(&segments(&them));
            let window = 2 * RATE as u64;
            let (mut echo_segs, mut echo_words, mut echo_secs) = (0, 0, 0.0);
            let me_words: usize = me_t.iter().map(|t| t.2.len()).sum();
            for (start, end, w) in &me_t {
                let near: Vec<String> = them_t
                    .iter()
                    .filter(|(s, e, _)| *s < end + window && *e + window > *start)
                    .flat_map(|t| t.2.iter().cloned())
                    .collect();
                if w.len() >= 3 && echo_overlap(w, &near) >= 0.5 {
                    echo_segs += 1;
                    echo_words += w.len();
                    echo_secs += (end - start) as f64 / RATE as f64;
                }
            }
            let them_words: usize = them_t.iter().map(|t| t.2.len()).sum();
            println!(
                "  me: {} segments, {me_words} words | them: {} segments, {them_words} words | echo: {echo_segs} me segments ({echo_secs:.0}s, {echo_words} words = {:.0}% of me words)",
                me_t.len(),
                them_t.len(),
                100.0 * echo_words as f64 / me_words.max(1) as f64
            );
        }
    }
}
