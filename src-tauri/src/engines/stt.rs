//! Speech-to-text dispatch: Whisper (a single `.bin` file) or Parakeet TDT
//! (a model directory), chosen from the model path under `models/stt/`.
//! Every engine returns text already passed through `postprocess::finalize`.

use anyhow::Result;
use std::path::Path;

use super::{parakeet, whisper};

/// True when `model` (a path or a bare id) names the Parakeet model.
pub fn is_parakeet(model: &str) -> bool {
    Path::new(model).file_name().and_then(|n| n.to_str()) == Some(parakeet::MODEL_ID)
}

/// Whether the model is on disk and complete enough to load.
pub fn is_available(model_path: &Path) -> bool {
    if is_parakeet(&model_path.to_string_lossy()) {
        parakeet::missing_files(model_path).is_empty()
    } else {
        model_path.exists()
    }
}

/// Load (and cache) the model so the first dictation doesn't pay for it.
pub fn preload(model_path: &str) -> Result<()> {
    if is_parakeet(model_path) {
        parakeet::get_or_load_cached(Path::new(model_path)).map(|_| ())
    } else {
        whisper::get_or_load_cached(model_path).map(|_| ())
    }
}

/// Transcribe mono 16 kHz audio.
pub fn transcribe(model_path: &str, audio: &[f32]) -> Result<String> {
    if is_parakeet(model_path) {
        parakeet::get_or_load_cached(Path::new(model_path))?.transcribe(audio)
    } else {
        whisper::get_or_load_cached(model_path)?.transcribe(audio)
    }
}

/// Like `transcribe`, reporting the text so far through `on_partial` as it
/// is decoded. Whisper reports per segment; Parakeet decodes in one pass.
pub fn transcribe_streaming(
    model_path: &str,
    audio: &[f32],
    mut on_partial: impl FnMut(String) + 'static,
) -> Result<String> {
    if is_parakeet(model_path) {
        parakeet::get_or_load_cached(Path::new(model_path))?.transcribe(audio)
    } else {
        let engine = whisper::get_or_load_cached(model_path)?;
        let mut accumulated = String::new();
        engine.transcribe_streaming(audio, move |data: whisper_rs::SegmentCallbackData| {
            accumulated.push_str(&data.text);
            on_partial(whisper::strip_non_speech(&accumulated));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parakeet_is_recognised_by_directory_name() {
        assert!(is_parakeet("parakeet-tdt-0.6b-v2"));
        assert!(is_parakeet("/x/models/stt/parakeet-tdt-0.6b-v2"));
        assert!(!is_parakeet("/x/models/stt/ggml-base.en.bin"));
        assert!(!is_parakeet("/x/parakeet-tdt-0.6b-v2/other"));
    }

    #[test]
    fn availability_needs_every_parakeet_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(parakeet::MODEL_ID);
        std::fs::create_dir_all(&p).unwrap();
        assert!(!is_available(&p));
        for (name, size, _) in parakeet::FILES {
            std::fs::File::create(p.join(name)).unwrap().set_len(size).unwrap();
        }
        assert!(is_available(&p));
        assert!(!is_available(&dir.path().join("ggml-base.en.bin")));
    }
}

/// Opt-in benchmark of Parakeet next to the downloaded Whisper models:
/// `STT_BENCH_WAVS=a.wav,b.wav cargo test --release --lib stt_real -- --ignored --nocapture`
/// (`PARAKEET_EP=coreml` switches Parakeet's encoder to CoreML.)
#[cfg(test)]
mod real_models {
    use super::*;

    #[test]
    #[ignore]
    fn stt_real_benchmark() {
        let wavs = std::env::var("STT_BENCH_WAVS").expect("set STT_BENCH_WAVS=a.wav,b.wav");
        let clips: Vec<(String, Vec<f32>)> = wavs
            .split(',')
            .map(|w| {
                let (samples, rate) = crate::audio::processing::read_wav(Path::new(w)).unwrap();
                (w.to_string(), crate::audio::capture::convert_rate(&samples, rate, 16_000))
            })
            .collect();
        let dir = dirs::data_dir().unwrap().join("com.blahcubed.app/models/stt");
        for model in [parakeet::MODEL_ID, "ggml-base.en.bin", "ggml-small.en.bin"] {
            let path = dir.join(model);
            if !is_available(&path) {
                println!("{model}: not downloaded");
                continue;
            }
            let path = path.to_string_lossy().to_string();
            let t0 = std::time::Instant::now();
            preload(&path).unwrap();
            println!("{model}: load {:?}", t0.elapsed());
            for (name, audio) in &clips {
                let _ = transcribe(&path, audio).unwrap(); // warm-up
                // Best and median of 5 runs: the machine is rarely idle.
                let mut times = Vec::new();
                let mut text = String::new();
                for _ in 0..5 {
                    let t1 = std::time::Instant::now();
                    text = transcribe(&path, audio).unwrap();
                    times.push(t1.elapsed());
                }
                times.sort();
                println!(
                    "{model} | {name} | {:.1}s audio | best {:?} median {:?} | {text}",
                    audio.len() as f32 / 16_000.0,
                    times[0],
                    times[2]
                );
            }
        }
    }
}
