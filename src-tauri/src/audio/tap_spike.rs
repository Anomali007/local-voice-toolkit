//! Phase 0 spike S1 (Blah³ Meetings): global Core Audio process tap that
//! excludes our own process → private aggregate device → IOProc → WAV.
//!
//! Not wired into the app. Two ways to run it:
//! - Signed build (TCC attributes the prompt to Blah³):
//!   `open -n --env BLAH3_TAP_SPIKE=60 --env BLAH3_TAP_SPIKE_OUT=<dir> Blah³.app`
//!   `main` runs the spike instead of the app and exits.
//! - Terminal (TCC attributes the prompt to the terminal app):
//!   `BLAH3_TAP_SPIKE_OUT=<dir> cargo test --lib tap_spike -- --ignored --nocapture`
//!
//! Writes `report.json` (format, callback counts, per-second dBFS, zero runs)
//! and, for runs of 120 s or less, `tap.wav` (the tap's native format).

use std::ffi::{c_void, CStr};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_core_audio::*;
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsNonInterleaved, AudioBufferList,
    AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};

const MAX_WAV_SECS: u64 = 120;

/// MacTypes `OSStatus` (the bindings keep their alias crate-private).
type OSStatus = i32;

/// Entry point for the signed-build path. Returns true when the spike ran
/// (the caller should exit instead of starting the app).
pub fn run_from_env() -> bool {
    let Ok(secs) = std::env::var("BLAH3_TAP_SPIKE") else {
        return false;
    };
    let secs: u64 = secs.parse().unwrap_or(60);
    let out = std::env::var("BLAH3_TAP_SPIKE_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("blah3-tap-spike"));
    if let Err(e) = run(secs, &out) {
        let _ = std::fs::create_dir_all(&out);
        let _ = std::fs::write(out.join("error.txt"), format!("{e:#}\n"));
    }
    true
}

/// Samples and counters shared with the IOProc (real-time thread).
#[derive(Default)]
struct Shared {
    samples: Mutex<Vec<f32>>,
    keep_samples: bool,
    callbacks: AtomicU64,
    zero_callbacks: AtomicU64,
    frames: AtomicU64,
    nonzero_frames: AtomicU64,
    first_nonzero_frame: AtomicU64,
    buffers_per_cb: AtomicU32,
    channels: AtomicU32,
    /// Sum of squares per second of audio (indexed by frame / sample rate).
    sec_sq: Mutex<Vec<(f64, u64)>>,
    sample_rate: AtomicU32,
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
    let list = input.as_ptr();
    let n_bufs = (*list).mNumberBuffers as usize;
    let bufs = std::slice::from_raw_parts((*list).mBuffers.as_ptr(), n_bufs);
    shared.callbacks.fetch_add(1, Ordering::Relaxed);
    shared.buffers_per_cb.store(n_bufs as u32, Ordering::Relaxed);
    if n_bufs == 0 || bufs[0].mData.is_null() {
        shared.zero_callbacks.fetch_add(1, Ordering::Relaxed);
        return 0;
    }

    // Interleave whatever layout arrives (one interleaved buffer, or one
    // buffer per channel) into a single f32 frame stream.
    let channels: usize = bufs.iter().map(|b| b.mNumberChannels as usize).sum();
    let frames = bufs[0].mDataByteSize as usize / 4 / bufs[0].mNumberChannels.max(1) as usize;
    shared.channels.store(channels as u32, Ordering::Relaxed);
    let mut chunk = Vec::with_capacity(frames * channels);
    for f in 0..frames {
        for b in bufs {
            let data = b.mData as *const f32;
            let ch = b.mNumberChannels as usize;
            for c in 0..ch {
                chunk.push(*data.add(f * ch + c));
            }
        }
    }

    let start_frame = shared.frames.fetch_add(frames as u64, Ordering::Relaxed);
    let all_zero = chunk.iter().all(|s| *s == 0.0);
    if all_zero {
        shared.zero_callbacks.fetch_add(1, Ordering::Relaxed);
    } else {
        shared.nonzero_frames.fetch_add(frames as u64, Ordering::Relaxed);
        let _ = shared.first_nonzero_frame.compare_exchange(
            u64::MAX,
            start_frame,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );
    }

    let rate = shared.sample_rate.load(Ordering::Relaxed).max(1) as u64;
    if let Ok(mut secs) = shared.sec_sq.lock() {
        let sec = (start_frame / rate) as usize;
        if secs.len() <= sec {
            secs.resize(sec + 1, (0.0, 0));
        }
        let sq: f64 = chunk.iter().map(|s| (*s as f64) * (*s as f64)).sum();
        secs[sec].0 += sq;
        secs[sec].1 += chunk.len() as u64;
    }
    if shared.keep_samples {
        if let Ok(mut s) = shared.samples.lock() {
            s.extend_from_slice(&chunk);
        }
    }
    0
}

fn ns(key: &CStr) -> Retained<NSString> {
    NSString::from_str(key.to_str().expect("ascii key"))
}

fn check(status: OSStatus, what: &str) -> Result<()> {
    if status != 0 {
        let b = status.to_be_bytes();
        let fourcc: String = b
            .iter()
            .map(|c| if c.is_ascii_graphic() { *c as char } else { '.' })
            .collect();
        bail!("{what} failed: OSStatus {status} ('{fourcc}')");
    }
    Ok(())
}

fn addr(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

/// Reads a fixed-size property. `qualifier` is passed through as raw bytes.
unsafe fn get_prop<T: Copy, Q>(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    qualifier: Option<&Q>,
    what: &str,
) -> Result<T> {
    let mut a = addr(selector);
    let mut out = std::mem::MaybeUninit::<T>::zeroed();
    let mut size = std::mem::size_of::<T>() as u32;
    let (q_size, q_ptr) = match qualifier {
        Some(q) => (std::mem::size_of::<Q>() as u32, q as *const Q as *const c_void),
        None => (0, std::ptr::null()),
    };
    check(
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut a),
            q_size,
            q_ptr,
            NonNull::from(&mut size),
            NonNull::new_unchecked(out.as_mut_ptr() as *mut c_void),
        ),
        what,
    )?;
    Ok(out.assume_init())
}

/// Byte size of a property in the given scope (0 if unreadable).
unsafe fn prop_size(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> u32 {
    let mut a = addr(selector);
    a.mScope = scope;
    let mut size = 0u32;
    let st = AudioObjectGetPropertyDataSize(
        object,
        NonNull::from(&mut a),
        0,
        std::ptr::null(),
        NonNull::from(&mut size),
    );
    if st == 0 {
        size
    } else {
        0
    }
}

/// Reads a CFString property (returned +1) as an NSString.
unsafe fn get_string_prop(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
    what: &str,
) -> Result<Retained<NSString>> {
    let ptr: *mut c_void = get_prop::<*mut c_void, ()>(object, selector, None, what)?;
    Retained::from_raw(ptr as *mut NSString).context(format!("{what} returned null"))
}

#[derive(serde::Serialize)]
struct Report {
    macos: String,
    pid: i32,
    own_process_object: u32,
    /// PID read back from `own_process_object` (should equal `pid`).
    own_process_object_pid: Option<i32>,
    excluded_self: bool,
    /// "sub" = aggregate with the default output as main sub-device (AudioCap's
    /// shape); "nosub" = tap only.
    mode: String,
    output_device_uid: String,
    agg_nominal_rate: f64,
    agg_input_streams: u32,
    agg_taps: u32,
    agg_active_subdevices: u32,
    agg_running_after_1s: Option<u32>,
    tap_format: Format,
    requested_secs: u64,
    elapsed_secs: f64,
    callbacks: u64,
    zero_callbacks: u64,
    buffers_per_callback: u32,
    channels_seen: u32,
    frames: u64,
    nonzero_frames: u64,
    first_nonzero_secs: Option<f64>,
    all_zero: bool,
    /// RMS per second of audio in dBFS (-120 = digital silence).
    per_second_dbfs: Vec<f64>,
    longest_silent_run_secs: u64,
    wav: Option<String>,
}

#[derive(serde::Serialize)]
struct Format {
    sample_rate: f64,
    format_id: String,
    format_flags: u32,
    float: bool,
    non_interleaved: bool,
    bits_per_channel: u32,
    channels_per_frame: u32,
    bytes_per_frame: u32,
}

fn fourcc(v: u32) -> String {
    String::from_utf8_lossy(&v.to_be_bytes()).into_owned()
}

pub fn run(secs: u64, out: &Path) -> Result<()> {
    let with_sub = std::env::var("BLAH3_TAP_SPIKE_MODE").as_deref() != Ok("nosub");
    std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))?;
    let pid = std::process::id() as i32;
    let system = kAudioObjectSystemObject as AudioObjectID;

    unsafe {
        // Our own process object, so the tap can exclude it. A process that has
        // never touched Core Audio may not have one yet (kAudioObjectUnknown).
        let own: AudioObjectID = get_prop(
            system,
            kAudioHardwarePropertyTranslatePIDToProcessObject,
            Some(&pid),
            "translate PID",
        )
        .unwrap_or(kAudioObjectUnknown);
        let own_pid = (own != kAudioObjectUnknown)
            .then(|| get_prop::<i32, ()>(own, kAudioProcessPropertyPID, None, "process PID").ok())
            .flatten();
        let exclude: Vec<Retained<NSNumber>> = if own != kAudioObjectUnknown {
            vec![NSNumber::new_u32(own)]
        } else {
            vec![]
        };
        let exclude = NSArray::from_retained_slice(&exclude);

        let desc = CATapDescription::initStereoGlobalTapButExcludeProcesses(
            CATapDescription::alloc(),
            &exclude,
        );
        desc.setName(&NSString::from_str("Blah³ tap spike"));
        desc.setPrivate(true);
        desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
        let tap_uid = desc.UUID().UUIDString();

        let mut tap: AudioObjectID = kAudioObjectUnknown;
        check(
            AudioHardwareCreateProcessTap(Some(&desc), &mut tap),
            "AudioHardwareCreateProcessTap",
        )?;

        let result = (|| -> Result<Report> {
            let asbd: AudioStreamBasicDescription =
                get_prop::<_, ()>(tap, kAudioTapPropertyFormat, None, "tap format")?;
            let out_dev: AudioObjectID = get_prop::<_, ()>(
                system,
                kAudioHardwarePropertyDefaultOutputDevice,
                None,
                "default output device",
            )?;
            let out_uid =
                get_string_prop(out_dev, kAudioDevicePropertyDeviceUID, "output device UID")?;

            let sub_device: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*ns(kAudioSubDeviceUIDKey)],
                &[&*out_uid as &AnyObject],
            );
            let sub_tap: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*ns(kAudioSubTapUIDKey), &*ns(kAudioSubTapDriftCompensationKey)],
                &[&*tap_uid as &AnyObject, &*NSNumber::new_bool(true)],
            );
            let agg_uid = NSUUID::new().UUIDString();
            let mut keys = vec![
                ns(kAudioAggregateDeviceNameKey),
                ns(kAudioAggregateDeviceUIDKey),
                ns(kAudioAggregateDeviceIsPrivateKey),
                ns(kAudioAggregateDeviceIsStackedKey),
                ns(kAudioAggregateDeviceTapAutoStartKey),
                ns(kAudioAggregateDeviceTapListKey),
            ];
            let name = NSString::from_str("Blah³ tap spike");
            let yes = NSNumber::new_bool(true);
            let no = NSNumber::new_bool(false);
            let subs = NSArray::from_retained_slice(&[sub_device]);
            let taps = NSArray::from_retained_slice(&[sub_tap]);
            let mut values: Vec<&AnyObject> = vec![&name, &agg_uid, &yes, &no, &yes, &taps];
            if with_sub {
                keys.push(ns(kAudioAggregateDeviceMainSubDeviceKey));
                values.push(&out_uid);
                keys.push(ns(kAudioAggregateDeviceSubDeviceListKey));
                values.push(&subs);
            }
            let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
            let dict: Retained<NSDictionary<NSString, AnyObject>> =
                NSDictionary::from_slices(&key_refs, &values);
            let cf_dict = &*(Retained::as_ptr(&dict) as *const CFDictionary);

            let mut agg: AudioObjectID = kAudioObjectUnknown;
            check(
                AudioHardwareCreateAggregateDevice(cf_dict, NonNull::from(&mut agg)),
                "AudioHardwareCreateAggregateDevice",
            )?;

            let agg_nominal_rate: f64 =
                get_prop::<_, ()>(agg, kAudioDevicePropertyNominalSampleRate, None, "agg rate")
                    .unwrap_or(0.0);
            let id_size = std::mem::size_of::<AudioObjectID>() as u32;
            let agg_input_streams =
                prop_size(agg, kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput)
                    / id_size;
            let cf_size = std::mem::size_of::<*const c_void>() as u32;
            let agg_taps = prop_size(
                agg,
                kAudioAggregateDevicePropertyTapList,
                kAudioObjectPropertyScopeGlobal,
            ) / cf_size;
            let agg_active_subdevices = prop_size(
                agg,
                kAudioAggregateDevicePropertyActiveSubDeviceList,
                kAudioObjectPropertyScopeGlobal,
            ) / id_size;
            let mut agg_running_after_1s = None;

            let shared = Box::new(Shared {
                keep_samples: secs <= MAX_WAV_SECS,
                first_nonzero_frame: AtomicU64::new(u64::MAX),
                sample_rate: AtomicU32::new(asbd.mSampleRate as u32),
                ..Default::default()
            });
            let shared_ptr = &*shared as *const Shared as *mut c_void;

            let run = (|| -> Result<f64> {
                let mut proc_id: AudioDeviceIOProcID = None;
                check(
                    AudioDeviceCreateIOProcID(
                        agg,
                        Some(io_proc),
                        shared_ptr,
                        NonNull::from(&mut proc_id),
                    ),
                    "AudioDeviceCreateIOProcID",
                )?;
                let started = Instant::now();
                let status = AudioDeviceStart(agg, proc_id);
                if status == 0 {
                    std::thread::sleep(Duration::from_secs(1));
                    agg_running_after_1s = get_prop::<u32, ()>(
                        agg,
                        kAudioDevicePropertyDeviceIsRunning,
                        None,
                        "is running",
                    )
                    .ok();
                    std::thread::sleep(Duration::from_secs(secs.saturating_sub(1)));
                    AudioDeviceStop(agg, proc_id);
                }
                AudioDeviceDestroyIOProcID(agg, proc_id);
                check(status, "AudioDeviceStart")?;
                Ok(started.elapsed().as_secs_f64())
            })();
            AudioHardwareDestroyAggregateDevice(agg);
            let elapsed = run?;

            let rate = asbd.mSampleRate.max(1.0);
            let per_second_dbfs: Vec<f64> = shared
                .sec_sq
                .lock()
                .unwrap()
                .iter()
                .map(|(sq, n)| {
                    if *n == 0 || *sq == 0.0 {
                        -120.0
                    } else {
                        (10.0 * (sq / *n as f64).log10()).max(-120.0)
                    }
                })
                .map(|d| (d * 10.0).round() / 10.0)
                .collect();
            let mut longest = 0u64;
            let mut cur = 0u64;
            for d in &per_second_dbfs {
                cur = if *d <= -120.0 { cur + 1 } else { 0 };
                longest = longest.max(cur);
            }

            let channels = shared.channels.load(Ordering::Relaxed);
            let wav = if shared.keep_samples && channels > 0 {
                let path = out.join("tap.wav");
                let spec = hound::WavSpec {
                    channels: channels as u16,
                    sample_rate: asbd.mSampleRate as u32,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let mut w = hound::WavWriter::create(&path, spec)?;
                for s in shared.samples.lock().unwrap().iter() {
                    w.write_sample(*s)?;
                }
                w.finalize()?;
                Some(path.display().to_string())
            } else {
                None
            };

            let first = shared.first_nonzero_frame.load(Ordering::Relaxed);
            let nonzero = shared.nonzero_frames.load(Ordering::Relaxed);
            Ok(Report {
                macos: macos_version(),
                pid,
                own_process_object: own,
                own_process_object_pid: own_pid,
                excluded_self: own != kAudioObjectUnknown,
                mode: if with_sub { "sub" } else { "nosub" }.into(),
                output_device_uid: out_uid.to_string(),
                agg_nominal_rate,
                agg_input_streams,
                agg_taps,
                agg_active_subdevices,
                agg_running_after_1s,
                tap_format: Format {
                    sample_rate: asbd.mSampleRate,
                    format_id: fourcc(asbd.mFormatID),
                    format_flags: asbd.mFormatFlags,
                    float: asbd.mFormatFlags & kAudioFormatFlagIsFloat != 0,
                    non_interleaved: asbd.mFormatFlags & kAudioFormatFlagIsNonInterleaved != 0,
                    bits_per_channel: asbd.mBitsPerChannel,
                    channels_per_frame: asbd.mChannelsPerFrame,
                    bytes_per_frame: asbd.mBytesPerFrame,
                },
                requested_secs: secs,
                elapsed_secs: (elapsed * 10.0).round() / 10.0,
                callbacks: shared.callbacks.load(Ordering::Relaxed),
                zero_callbacks: shared.zero_callbacks.load(Ordering::Relaxed),
                buffers_per_callback: shared.buffers_per_cb.load(Ordering::Relaxed),
                channels_seen: channels,
                frames: shared.frames.load(Ordering::Relaxed),
                nonzero_frames: nonzero,
                first_nonzero_secs: (first != u64::MAX)
                    .then(|| ((first as f64 / rate) * 10.0).round() / 10.0),
                all_zero: nonzero == 0,
                per_second_dbfs,
                longest_silent_run_secs: longest,
                wav,
            })
        })();
        AudioHardwareDestroyProcessTap(tap);

        let report = result?;
        std::fs::write(
            out.join("report.json"),
            serde_json::to_string_pretty(&report)? + "\n",
        )?;
    }
    Ok(())
}

fn macos_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    /// Runs from the terminal, so TCC asks on behalf of the terminal app, not
    /// Blah³. Use the signed-build path for the real permission check.
    #[test]
    #[ignore]
    fn tap_spike() {
        let out = std::env::var("BLAH3_TAP_SPIKE_OUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("blah3-tap-spike"));
        let secs = std::env::var("BLAH3_TAP_SPIKE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        super::run(secs, &out).unwrap();
        println!("{}", std::fs::read_to_string(out.join("report.json")).unwrap());
    }
}
