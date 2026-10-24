//! Start / stop / cancel cues, played with the macOS system sounds in
//! `/System/Library/Sounds` through `afplay` (non-blocking child process;
//! rodio is built wav-only and the system sounds are AIFF).
//!
//! Timing (so a cue is never transcribed):
//! - Start: the microphone is opened FIRST, then the chime plays (so it means
//!   "go"), and when it finishes everything captured so far is discarded
//!   (`AudioCapture::discard_buffered`). The mic hears the speakers, so this
//!   removes the chime from the audio; the cost is that words spoken during
//!   the ~0.2-1 s chime are lost, same as waiting for the beep.
//! - Stop/cancel: played only AFTER capture has been stopped and its buffer
//!   taken, so the cue cannot leak into the recording.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::commands::settings::AppSettings;

const SOUND_DIR: &str = "/System/Library/Sounds";
/// Don't wait longer than this for a cue to finish before cutting it out.
const MAX_CUE_WAIT: Duration = Duration::from_millis(1500);

/// Names (file stems) of the system sounds, sorted.
pub fn available_sounds() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(SOUND_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            (path.extension()?.to_str()? == "aiff").then(|| path.file_stem()?.to_str().map(String::from))?
        })
        .collect();
    names.sort();
    names
}

/// A sound name is only ever a plain file stem; anything with a separator or
/// odd character is refused so settings can't point afplay elsewhere.
fn sound_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')) {
        return None;
    }
    let path = PathBuf::from(SOUND_DIR).join(format!("{name}.aiff"));
    path.is_file().then_some(path)
}

fn volume_arg(volume: f32) -> Option<String> {
    let v = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.5 };
    (v > 0.0).then(|| format!("{v:.2}"))
}

/// Start playing; returns the child so a caller can wait for it.
fn spawn(name: &str, volume: f32) -> Option<std::process::Child> {
    let path = sound_path(name)?;
    let vol = volume_arg(volume)?;
    Command::new("/usr/bin/afplay")
        .arg("-v")
        .arg(vol)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|e| tracing::debug!("afplay failed to start: {}", e))
        .ok()
}

/// Fire and forget (a reaper thread collects the child so it can't zombie).
pub fn play(name: &str, volume: f32) {
    if let Some(mut child) = spawn(name, volume) {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Play and block until it ends (or `MAX_CUE_WAIT`). Call off the UI thread.
pub fn play_and_wait(name: &str, volume: f32) {
    if let Some(mut child) = spawn(name, volume) {
        let started = Instant::now();
        while started.elapsed() < MAX_CUE_WAIT {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Settings for the start cue, if enabled.
pub fn start_cue(settings: &AppSettings) -> Option<(String, f32)> {
    settings.sound_on_start.then(|| (settings.sound_start_name.clone(), settings.sound_volume))
}

/// Stop cue (after a dictation ends normally).
pub fn play_stop(settings: &AppSettings) {
    if settings.sound_on_stop {
        play(&settings.sound_stop_name, settings.sound_volume);
    }
}

/// Subtle cancel cue: a fixed soft sound at 60% of the chosen volume, gated
/// by the same "sound on stop" switch.
pub fn play_cancel(settings: &AppSettings) {
    if settings.sound_on_stop {
        play("Bottle", settings.sound_volume * 0.6);
    }
}

#[tauri::command]
pub fn list_system_sounds() -> Vec<String> {
    available_sounds()
}

#[tauri::command]
pub fn preview_sound(name: String, volume: f32) -> Result<(), String> {
    if sound_path(&name).is_none() {
        return Err(format!("Unknown sound: {name}"));
    }
    play(&name, volume);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_escape_the_sound_directory() {
        for bad in ["", "../../etc/passwd", "/etc/passwd", "Tink/../x", "a.b", "Tink;ls", "Tink\0"] {
            assert!(sound_path(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn volume_is_clamped_and_zero_means_silent() {
        assert_eq!(volume_arg(0.5).as_deref(), Some("0.50"));
        assert_eq!(volume_arg(5.0).as_deref(), Some("1.00"));
        assert_eq!(volume_arg(-1.0), None);
        assert_eq!(volume_arg(0.0), None);
        assert_eq!(volume_arg(f32::NAN).as_deref(), Some("0.50"));
    }

    #[test]
    fn default_choices_exist_on_this_mac() {
        let s = AppSettings::default();
        let found = available_sounds();
        if cfg!(target_os = "macos") && !found.is_empty() {
            assert!(found.contains(&s.sound_start_name), "{found:?}");
            assert!(found.contains(&s.sound_stop_name), "{found:?}");
            assert!(found.contains(&"Bottle".to_string()));
            assert!(sound_path("Tink").is_some());
        }
    }

    #[test]
    fn cues_follow_the_switches() {
        let mut s = AppSettings::default();
        assert!(start_cue(&s).is_none());
        s.sound_on_start = true;
        assert_eq!(start_cue(&s).unwrap().0, "Tink");
    }
}
