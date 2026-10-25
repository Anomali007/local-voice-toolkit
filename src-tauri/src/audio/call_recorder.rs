//! Phase 0 spike S4 (Blah³ Meetings): records a real call as two aligned
//! streams, the default microphone ("Me") and the system-audio tap ("Them"),
//! so the echo check in `meetings::echo` can measure how much of the far side
//! comes back through the mic when it plays on speakers.
//!
//! Not wired into the app. Signed-build path (TCC attributes both prompts to
//! Blah³):
//!   `open -n --env BLAH3_S4=<max secs> --env BLAH3_S4_OUT=<dir> Blah³.app`
//! `touch <dir>/stop` ends the recording early.
//!
//! Writes `me.wav` and `them.wav` (16 kHz mono f32, same timeline) and
//! `report.json`: devices and formats over time, tap gaps, per-second dBFS.
//! The WAVs hold the call itself: keep them out of the repo and delete them
//! once measured.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use objc2_core_audio::*;
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};

use super::capture::{convert_rate, AudioCapture, SilenceConfig};
use super::tap_spike::{
    check, create_tap_device, fourcc, get_prop, get_string_prop, macos_version, OSStatus,
};

const RATE: u32 = 16_000;
/// A tap chunk arriving later than this past the end of the audio so far
/// starts a new run (the aggregate idles while nothing plays).
const GAP_SECS: f64 = 0.05;
const POLL: Duration = Duration::from_millis(250);

/// Entry point for the signed-build path. Returns true when the recorder ran
/// (the caller should exit instead of starting the app).
pub fn run_from_env() -> bool {
    let Ok(secs) = std::env::var("BLAH3_S4") else {
        return false;
    };
    let secs: u64 = secs.parse().unwrap_or(600);
    let out = std::env::var("BLAH3_S4_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("blah3-s4"));
    if let Err(e) = run(secs, &out) {
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(out.join("error.txt"), format!("{e:#}\n"));
    }
    true
}

/// Tap audio shared with the IOProc (real-time thread).
struct Shared {
    t0: Instant,
    /// (arrival secs since t0, sample rate, mono samples)
    chunks: Mutex<Vec<(f64, u32, Vec<f32>)>>,
    rate: AtomicU32,
    callbacks: AtomicU64,
}

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> OSStatus {
    let shared = &*(client as *const Shared);
    let arrived = shared.t0.elapsed().as_secs_f64();
    shared.callbacks.fetch_add(1, Ordering::Relaxed);
    let list = input.as_ptr();
    let n_bufs = (*list).mNumberBuffers as usize;
    let bufs = std::slice::from_raw_parts((*list).mBuffers.as_ptr(), n_bufs);
    if n_bufs == 0 || bufs[0].mData.is_null() {
        return 0;
    }
    // Downmix whatever layout arrives (interleaved, or a buffer per channel).
    let channels: usize = bufs.iter().map(|b| b.mNumberChannels as usize).sum();
    let frames = bufs[0].mDataByteSize as usize / 4 / bufs[0].mNumberChannels.max(1) as usize;
    let mut mono = vec![0f32; frames];
    for b in bufs {
        let data = b.mData as *const f32;
        let ch = b.mNumberChannels as usize;
        for (f, m) in mono.iter_mut().enumerate() {
            for c in 0..ch {
                *m += *data.add(f * ch + c);
            }
        }
    }
    let scale = 1.0 / channels.max(1) as f32;
    mono.iter_mut().for_each(|m| *m *= scale);
    let rate = shared.rate.load(Ordering::Relaxed);
    if let Ok(mut chunks) = shared.chunks.lock() {
        chunks.push((arrived, rate, mono));
    }
    0
}

/// What the default devices and the tap look like at one moment. A new
/// entry is logged only when something changes.
#[derive(serde::Serialize, Clone, PartialEq)]
struct Snapshot {
    input_name: String,
    input_transport: String,
    input_rate: f64,
    output_name: String,
    output_transport: String,
    output_rate: f64,
    /// What the tap says it produces. Not what the IOProc gets: the buffers
    /// arrive at the aggregate's rate (with AirPods on their mic, 24 kHz
    /// while this still says 48 kHz).
    tap_rate: f64,
    tap_channels: u32,
    agg_rate: f64,
}

#[derive(serde::Serialize)]
struct TimelineEntry {
    t: f64,
    #[serde(flatten)]
    snapshot: Snapshot,
}

#[derive(serde::Serialize)]
struct Report {
    macos: String,
    requested_secs: u64,
    elapsed_secs: f64,
    stopped_by: String,
    excluded_self: bool,
    mic_samples: usize,
    mic_start_secs: f64,
    tap_callbacks: u64,
    tap_chunks: usize,
    /// Stretches of continuous tap audio (a new one after each idle gap or
    /// rate change).
    tap_runs: usize,
    tap_rates_seen: Vec<u32>,
    tap_audio_secs: f64,
    /// Frames per second actually delivered during long unbroken stretches;
    /// should equal the rate the chunks were labelled with.
    tap_measured_rate: Option<f64>,
    timeline: Vec<TimelineEntry>,
    me_dbfs: Vec<f64>,
    them_dbfs: Vec<f64>,
}

unsafe fn device_snapshot(tap: AudioObjectID, agg: AudioObjectID) -> Snapshot {
    let system = kAudioObjectSystemObject as AudioObjectID;
    let describe = |selector| -> (String, String, f64) {
        let Ok(dev) = get_prop::<AudioObjectID, ()>(system, selector, None, "default device")
        else {
            return ("?".into(), "?".into(), 0.0);
        };
        let name = get_string_prop(dev, kAudioObjectPropertyName, "device name")
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "?".into());
        let transport =
            get_prop::<u32, ()>(dev, kAudioDevicePropertyTransportType, None, "transport")
                .map(fourcc)
                .unwrap_or_else(|_| "?".into());
        let rate = get_prop::<f64, ()>(dev, kAudioDevicePropertyNominalSampleRate, None, "rate")
            .unwrap_or(0.0);
        (name, transport.trim().to_string(), rate)
    };
    let (input_name, input_transport, input_rate) =
        describe(kAudioHardwarePropertyDefaultInputDevice);
    let (output_name, output_transport, output_rate) =
        describe(kAudioHardwarePropertyDefaultOutputDevice);
    let fmt =
        get_prop::<AudioStreamBasicDescription, ()>(tap, kAudioTapPropertyFormat, None, "tap").ok();
    Snapshot {
        input_name,
        input_transport,
        input_rate,
        output_name,
        output_transport,
        output_rate,
        tap_rate: fmt.map(|f| f.mSampleRate).unwrap_or(0.0),
        tap_channels: fmt.map(|f| f.mChannelsPerFrame).unwrap_or(0),
        agg_rate: get_prop::<f64, ()>(agg, kAudioDevicePropertyNominalSampleRate, None, "agg rate")
            .unwrap_or(0.0),
    }
}

/// Lays tap chunks on the t0 timeline at 16 kHz. Consecutive chunks at one
/// rate form a run that is resampled as a whole; a run starts where its
/// first chunk's arrival says it does, so idle gaps become silence.
fn assemble_tap(chunks: &[(f64, u32, Vec<f32>)]) -> (Vec<f32>, usize) {
    let mut out: Vec<f32> = Vec::new();
    let mut runs = 0;
    let mut i = 0;
    while i < chunks.len() {
        let (arrived, rate, first) = &chunks[i];
        let rate = (*rate).max(1);
        let start = (arrived - first.len() as f64 / rate as f64).max(0.0);
        let mut native: Vec<f32> = Vec::new();
        let mut j = i;
        while j < chunks.len() && chunks[j].1.max(1) == rate {
            let end_so_far = start + (native.len() + chunks[j].2.len()) as f64 / rate as f64;
            if j > i && chunks[j].0 - end_so_far > GAP_SECS {
                break;
            }
            native.extend_from_slice(&chunks[j].2);
            j += 1;
        }
        let audio = convert_rate(&native, rate, RATE);
        let at = ((start * RATE as f64) as usize).max(out.len());
        out.resize(at, 0.0);
        out.extend_from_slice(&audio);
        runs += 1;
        i = j;
    }
    (out, runs)
}

/// Frames per wall-clock second over stretches of at least 100 callbacks
/// with no idle gap (the label the audio is resampled from must match).
fn measured_rate(chunks: &[(f64, u32, Vec<f32>)]) -> Option<f64> {
    let (mut frames, mut secs) = (0f64, 0f64);
    let mut i = 0;
    while i < chunks.len() {
        let mut j = i + 1;
        while j < chunks.len() && chunks[j].0 - chunks[j - 1].0 < 0.1 {
            j += 1;
        }
        if j - i >= 100 {
            // Frames delivered after the first arrival, over the time since.
            frames += chunks[i + 1..j]
                .iter()
                .map(|c| c.2.len() as f64)
                .sum::<f64>();
            secs += chunks[j - 1].0 - chunks[i].0;
        }
        i = j;
    }
    (secs > 1.0).then(|| (frames / secs).round())
}

fn per_second_dbfs(x: &[f32]) -> Vec<f64> {
    x.chunks(RATE as usize)
        .map(|c| {
            let ms = c.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / c.len() as f64;
            let db = if ms > 0.0 {
                (10.0 * ms.log10()).max(-120.0)
            } else {
                -120.0
            };
            (db * 10.0).round() / 10.0
        })
        .collect()
}

fn write_wav(path: &Path, x: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    for s in x {
        w.write_sample(*s)?;
    }
    w.finalize()?;
    Ok(())
}

pub fn run(secs: u64, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))?;
    let stop_file = out.join("stop");
    let _ = std::fs::remove_file(&stop_file);

    let mic = AudioCapture::with_silence_config(SilenceConfig {
        enabled: false,
        ..Default::default()
    })?;
    let t0 = Instant::now();
    mic.start()?;
    let mic_start_secs = t0.elapsed().as_secs_f64();

    let shared = Box::new(Shared {
        t0,
        chunks: Mutex::new(Vec::new()),
        rate: AtomicU32::new(0),
        callbacks: AtomicU64::new(0),
    });
    let shared_ptr = &*shared as *const Shared as *mut c_void;

    let mut timeline = Vec::new();
    let (stopped_by, excluded_self, elapsed) = unsafe {
        let dev = create_tap_device("Blah³ S4 recorder", true)?;
        let first = device_snapshot(dev.tap, dev.agg);
        let rate = if first.agg_rate > 0.0 {
            first.agg_rate
        } else {
            dev.format.mSampleRate
        };
        shared.rate.store(rate as u32, Ordering::Relaxed);
        let mut proc_id: AudioDeviceIOProcID = None;
        check(
            AudioDeviceCreateIOProcID(
                dev.agg,
                Some(io_proc),
                shared_ptr,
                NonNull::from(&mut proc_id),
            ),
            "AudioDeviceCreateIOProcID",
        )?;
        let status = AudioDeviceStart(dev.agg, proc_id);
        let mut stopped_by = "duration";
        if status == 0 {
            let mut last: Option<Snapshot> = None;
            while t0.elapsed() < Duration::from_secs(secs) {
                let snap = device_snapshot(dev.tap, dev.agg);
                if snap.agg_rate > 0.0 {
                    shared.rate.store(snap.agg_rate as u32, Ordering::Relaxed);
                }
                if last.as_ref() != Some(&snap) {
                    timeline.push(TimelineEntry {
                        t: (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                        snapshot: snap.clone(),
                    });
                    last = Some(snap);
                }
                if stop_file.exists() {
                    stopped_by = "stop file";
                    break;
                }
                std::thread::sleep(POLL);
            }
            AudioDeviceStop(dev.agg, proc_id);
        }
        AudioDeviceDestroyIOProcID(dev.agg, proc_id);
        check(status, "AudioDeviceStart")?;
        (
            stopped_by,
            dev.own_pid.is_some(),
            t0.elapsed().as_secs_f64(),
        )
    };
    let mic_audio = mic.stop()?;

    let chunks = shared.chunks.lock().unwrap();
    let (mut them, tap_runs) = assemble_tap(&chunks);
    let mut me = vec![0f32; (mic_start_secs * RATE as f64) as usize];
    me.extend_from_slice(&mic_audio);
    let len = me.len().max(them.len());
    me.resize(len, 0.0);
    them.resize(len, 0.0);

    write_wav(&out.join("me.wav"), &me)?;
    write_wav(&out.join("them.wav"), &them)?;

    let mut rates: Vec<u32> = chunks.iter().map(|c| c.1).collect();
    rates.sort_unstable();
    rates.dedup();
    let tap_frames: f64 = chunks
        .iter()
        .map(|c| c.2.len() as f64 / c.1.max(1) as f64)
        .sum();
    let report = Report {
        macos: macos_version(),
        requested_secs: secs,
        elapsed_secs: (elapsed * 10.0).round() / 10.0,
        stopped_by: stopped_by.into(),
        excluded_self,
        mic_samples: mic_audio.len(),
        mic_start_secs: (mic_start_secs * 1000.0).round() / 1000.0,
        tap_callbacks: shared.callbacks.load(Ordering::Relaxed),
        tap_chunks: chunks.len(),
        tap_runs,
        tap_rates_seen: rates,
        tap_audio_secs: (tap_frames * 10.0).round() / 10.0,
        tap_measured_rate: measured_rate(&chunks),
        timeline,
        me_dbfs: per_second_dbfs(&me),
        them_dbfs: per_second_dbfs(&them),
    };
    std::fs::write(
        out.join("report.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_runs_land_where_they_arrived() {
        // 48 kHz: two chunks back to back, a 1 s idle gap, one more chunk.
        let c = |t: f64| (t, 48_000, vec![0.5f32; 4800]); // 0.1 s each
        let chunks = vec![c(0.1), c(0.2), c(1.3)];
        let (out, runs) = assemble_tap(&chunks);
        assert_eq!(runs, 2);
        // First run covers 0.0–0.2 s, silence until 1.2 s, then 0.1 s.
        assert_eq!(out.len(), (1.3 * RATE as f64) as usize);
        assert_eq!(out[100], 0.5);
        assert_eq!(out[(0.7 * RATE as f64) as usize], 0.0);
        assert_eq!(out[(1.25 * RATE as f64) as usize], 0.5);
    }

    #[test]
    fn rate_change_starts_a_new_run_at_the_right_length() {
        let chunks = vec![
            (0.1, 48_000, vec![0.25f32; 4800]),
            (0.2, 24_000, vec![0.25f32; 2400]),
        ];
        let (out, runs) = assemble_tap(&chunks);
        assert_eq!(runs, 2);
        assert_eq!(out.len(), (0.2 * RATE as f64) as usize);
        assert!(out.iter().all(|s| (*s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn measured_rate_reads_what_actually_arrived() {
        // 480-frame buffers every 20 ms = 24 kHz, whatever the label says.
        let chunks: Vec<_> = (0..300)
            .map(|k| (k as f64 * 0.02, 48_000, vec![0f32; 480]))
            .collect();
        assert_eq!(measured_rate(&chunks), Some(24_000.0));
        assert_eq!(measured_rate(&chunks[..50]), None);
    }

    #[test]
    fn jitter_does_not_split_a_run() {
        // Arrivals wobble by 20 ms around a steady 0.1 s cadence.
        let chunks: Vec<_> = (1..=10)
            .map(|k| {
                (
                    k as f64 * 0.1 + if k % 2 == 0 { 0.02 } else { -0.02 },
                    48_000,
                    vec![0.1f32; 4800],
                )
            })
            .collect();
        let (out, runs) = assemble_tap(&chunks);
        assert_eq!(runs, 1);
        assert_eq!(out.len(), RATE as usize);
    }
}
