//! Pitch-preserving time stretch (WSOLA).
//!
//! Changing the playback sample rate speeds speech up but also raises its
//! pitch. WSOLA instead cuts the signal into overlapping windows and places
//! each one where it best lines up with the previous, so the voice keeps its
//! pitch at any speed.

/// Window length in seconds (~40 ms suits speech).
const WINDOW_SECS: f32 = 0.04;
/// How far each window may move to find the best alignment, in seconds.
const TOLERANCE_SECS: f32 = 0.01;

pub const MIN_SPEED: f32 = 0.5;
pub const MAX_SPEED: f32 = 3.0;

/// Stretch `samples` (mono) so they play `speed` times faster, keeping pitch.
pub fn time_stretch(samples: &[f32], sample_rate: u32, speed: f32) -> Vec<f32> {
    let speed = if speed.is_finite() { speed.clamp(MIN_SPEED, MAX_SPEED) } else { 1.0 };
    let n = ((sample_rate as f32 * WINDOW_SECS) as usize).max(64) & !1;
    if (speed - 1.0).abs() < 0.01 || samples.len() < n {
        return samples.to_vec();
    }

    let hs = n / 2;
    let ha = hs as f32 * speed;
    let tol = (sample_rate as f32 * TOLERANCE_SECS) as isize;

    // Pad so the first and last windows see full context.
    let mut x = vec![0.0f32; n];
    x.extend_from_slice(samples);
    x.extend(std::iter::repeat_n(0.0, n + tol as usize));

    let window: Vec<f32> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
        .collect();

    let frames = ((x.len() - n) as f32 / ha) as usize;
    let mut out = vec![0.0f32; frames * hs + n];
    let mut prev = 0usize;

    for k in 0..frames {
        let nominal = (k as f32 * ha) as isize;
        let pos = if k == 0 {
            0
        } else {
            // Match against the natural continuation of the previous window.
            let template = prev + hs;
            if template + n > x.len() {
                break;
            }
            let mut best = nominal;
            let mut best_score = f32::MIN;
            for delta in (-tol..=tol).step_by(2) {
                let cand = nominal + delta;
                if cand < 0 || cand as usize + n > x.len() {
                    continue;
                }
                let cand = cand as usize;
                let mut score = 0.0;
                for i in (0..hs).step_by(2) {
                    score += x[template + i] * x[cand + i];
                }
                if score > best_score {
                    best_score = score;
                    best = cand as isize;
                }
            }
            best as usize
        };
        if pos + n > x.len() {
            break;
        }
        let o = k * hs;
        for i in 0..n {
            out[o + i] += x[pos + i] * window[i];
        }
        prev = pos;
    }

    // Drop the front padding (n input samples = n / speed output samples).
    let start = (n as f32 / speed).round() as usize;
    let len = (samples.len() as f32 / speed).round() as usize;
    out.into_iter().skip(start).take(len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 24000;

    fn tone(freq: f32, secs: f32) -> Vec<f32> {
        (0..(SR as f32 * secs) as usize)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin())
            .collect()
    }

    /// Dominant frequency estimate from upward zero crossings.
    fn pitch(samples: &[f32]) -> f32 {
        let s = &samples[samples.len() / 10..samples.len() * 9 / 10];
        let crossings = s.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        crossings as f32 * SR as f32 / s.len() as f32
    }

    #[test]
    fn speed_one_is_identity() {
        let x = tone(220.0, 0.5);
        assert_eq!(time_stretch(&x, SR, 1.0), x);
    }

    #[test]
    fn duration_scales_with_speed() {
        let x = tone(220.0, 2.0);
        for speed in [0.5, 0.75, 1.25, 1.5, 2.0, 3.0] {
            let y = time_stretch(&x, SR, speed);
            let expected = x.len() as f32 / speed;
            let err = (y.len() as f32 - expected).abs() / expected;
            assert!(err < 0.01, "speed {speed}: {} samples, expected {expected}", y.len());
        }
    }

    #[test]
    fn pitch_is_preserved() {
        let x = tone(220.0, 2.0);
        assert!((pitch(&x) - 220.0).abs() < 3.0, "baseline pitch {}", pitch(&x));
        for speed in [0.5, 1.5, 2.0] {
            let p = pitch(&time_stretch(&x, SR, speed));
            assert!((p - 220.0).abs() < 220.0 * 0.03, "speed {speed}: pitch {p} Hz");
        }
    }

    #[test]
    fn pitch_check_catches_resampling() {
        // Negative control: the old approach (play at sample_rate * speed) is
        // what pitch_is_preserved must reject. Simulate it by decimating.
        let x = tone(220.0, 2.0);
        let resampled: Vec<f32> = x.iter().step_by(2).copied().collect();
        assert!((pitch(&resampled) - 440.0).abs() < 10.0);
    }

    #[test]
    fn output_is_not_silent_and_stays_bounded() {
        let x = tone(220.0, 1.0);
        let y = time_stretch(&x, SR, 1.5);
        let rms = (y.iter().map(|v| v * v).sum::<f32>() / y.len() as f32).sqrt();
        assert!(rms > 0.25, "rms {rms}");
        assert!(y.iter().all(|v| v.abs() <= 0.75));
    }

    #[test]
    fn short_empty_and_bad_input() {
        assert!(time_stretch(&[], SR, 1.5).is_empty());
        let short = vec![0.1; 10];
        assert_eq!(time_stretch(&short, SR, 2.0), short);
        let x = tone(220.0, 0.5);
        assert_eq!(time_stretch(&x, SR, f32::NAN), x);
        // Out-of-range speeds clamp instead of exploding.
        let y = time_stretch(&x, SR, 100.0);
        assert!((y.len() as f32 - x.len() as f32 / MAX_SPEED).abs() < 100.0);
    }
}
