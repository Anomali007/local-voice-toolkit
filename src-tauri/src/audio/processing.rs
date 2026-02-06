//! Audio processing utilities.
//!
//! These functions are available for future use in audio manipulation.

#![allow(dead_code)]

use anyhow::Result;

/// Convert audio samples from one sample rate to another
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let new_len = (samples.len() as f64 * ratio) as usize;
    let mut resampled = Vec::with_capacity(new_len);

    for i in 0..new_len {
        let src_idx = i as f64 / ratio;
        let src_idx_floor = src_idx.floor() as usize;
        let src_idx_ceil = (src_idx_floor + 1).min(samples.len() - 1);
        let frac = src_idx - src_idx_floor as f64;

        let sample = samples[src_idx_floor] * (1.0 - frac as f32)
            + samples[src_idx_ceil] * frac as f32;
        resampled.push(sample);
    }

    resampled
}

/// Convert stereo audio to mono by averaging channels
pub fn stereo_to_mono(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks(2)
        .map(|chunk| {
            if chunk.len() == 2 {
                (chunk[0] + chunk[1]) / 2.0
            } else {
                chunk[0]
            }
        })
        .collect()
}

/// Normalize audio samples to [-1.0, 1.0] range
pub fn normalize(samples: &mut [f32]) {
    let max = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if max > 0.0 && max != 1.0 {
        for sample in samples.iter_mut() {
            *sample /= max;
        }
    }
}

/// Convert i16 PCM samples to f32
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect()
}

/// Convert f32 samples to i16 PCM
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16)
        .collect()
}

/// Write audio samples to a WAV file
pub fn write_wav(path: &std::path::Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude)?;
    }
    writer.finalize()?;

    Ok(())
}

/// Read audio samples from a WAV file
pub fn read_wav(path: &std::path::Path) -> Result<(Vec<f32>, u32)> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
        hound::SampleFormat::Int => reader
            .into_samples::<i16>()
            .filter_map(Result::ok)
            .map(|s| s as f32 / i16::MAX as f32)
            .collect(),
    };

    // Convert to mono if stereo
    let samples = if spec.channels == 2 {
        stereo_to_mono(&samples)
    } else {
        samples
    };

    Ok((samples, sample_rate))
}
