//! Local dictation history: one JSON object per line in
//! `<app data>/com.blahcubed.app/history.jsonl`.
//!
//! Privacy: the file holds transcripts, so it is created owner-only (0600) and
//! kept that way, never leaves the machine, and its text is NEVER written to
//! the log (only counts). Capped at the newest N entries; can be turned off
//! or wiped from Settings.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::accessibility::FrontmostAppInfo;
use crate::commands::settings::AppSettings;

const MIN_ENTRIES: usize = 10;
const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub id: String,
    pub text: String,
    /// Unix epoch, milliseconds.
    pub timestamp_ms: u64,
    /// Length of the recorded audio (pauses excluded), milliseconds.
    pub duration_ms: u64,
    pub app_name: Option<String>,
    pub app_bundle_id: Option<String>,
    pub model: String,
}

/// Serialises every read-modify-write of the file.
static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn history_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("history.jsonl")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn new_id(ts: u64) -> String {
    format!("{:x}-{:x}", ts, COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn clamp_cap(n: u32) -> usize {
    (n as usize).clamp(MIN_ENTRIES, MAX_ENTRIES)
}

/// Make sure an existing file is not readable by others (e.g. created by an
/// older build or restored from a backup with looser bits).
fn ensure_private(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.permissions().mode() & 0o077 != 0 {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

fn read_all(path: &Path) -> Vec<HistoryEntry> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        .collect()
}

/// Atomically replace the file with `entries` (temp file 0600 + rename).
fn write_all(path: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)?;
        for e in entries {
            let line = serde_json::to_string(e).map_err(std::io::Error::other)?;
            f.write_all(line.as_bytes())?;
            f.write_all(b"\n")?;
        }
        f.flush()?;
    }
    ensure_private(&tmp);
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Append `entry`, then drop the oldest entries beyond `cap`.
pub fn append_at(path: &Path, entry: &HistoryEntry, cap: usize) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
    }
    ensure_private(path);
    // Newline count is a cheap upper bound on entries; only parse when over.
    let lines = std::fs::read(path).map(|b| b.iter().filter(|&&c| c == b'\n').count()).unwrap_or(0);
    if lines > cap {
        let all = read_all(path);
        if all.len() > cap {
            write_all(path, &all[all.len() - cap..])?;
        }
    }
    Ok(())
}

/// Newest first.
pub fn load_at(path: &Path) -> Vec<HistoryEntry> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = read_all(path);
    all.reverse();
    all
}

pub fn get_at(path: &Path, id: &str) -> Option<HistoryEntry> {
    load_at(path).into_iter().find(|e| e.id == id)
}

/// Returns whether anything was removed.
pub fn delete_at(path: &Path, id: &str) -> std::io::Result<bool> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let all = read_all(path);
    let kept: Vec<HistoryEntry> = all.iter().filter(|e| e.id != id).cloned().collect();
    if kept.len() == all.len() {
        return Ok(false);
    }
    write_all(path, &kept)?;
    Ok(true)
}

pub fn clear_at(path: &Path) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Record a finished dictation. No-op when history is off or the text is
/// empty. Logs counts only.
pub fn record(
    app: &AppHandle,
    settings: &AppSettings,
    text: &str,
    audio_samples_16k: usize,
    target: Option<&FrontmostAppInfo>,
) {
    if !settings.history_enabled || text.trim().is_empty() {
        return;
    }
    let ts = now_ms();
    let entry = HistoryEntry {
        id: new_id(ts),
        text: text.to_string(),
        timestamp_ms: ts,
        duration_ms: (audio_samples_16k as u64) * 1000 / 16_000,
        app_name: target.map(|a| a.name.clone()).filter(|n| !n.is_empty()),
        app_bundle_id: target.map(|a| a.bundle_id.clone()).filter(|n| !n.is_empty()),
        model: settings.stt_model.clone(),
    };
    let cap = clamp_cap(settings.history_max_entries);
    match append_at(&history_path(), &entry, cap) {
        Ok(()) => {
            tracing::debug!("History: saved entry ({} chars)", text.chars().count());
            let _ = app.emit("history-updated", ());
        }
        Err(e) => tracing::warn!("History: could not save entry: {}", e),
    }
}

#[tauri::command]
pub fn get_history() -> Vec<HistoryEntry> {
    load_at(&history_path())
}

#[tauri::command]
pub fn delete_history_entry(app: AppHandle, id: String) -> Result<(), String> {
    delete_at(&history_path(), &id).map_err(|e| format!("Could not delete the entry: {e}"))?;
    let _ = app.emit("history-updated", ());
    Ok(())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    clear_at(&history_path()).map_err(|e| format!("Could not clear history: {e}"))?;
    tracing::info!("History cleared");
    let _ = app.emit("history-updated", ());
    Ok(())
}

#[tauri::command]
pub fn copy_history_entry(id: String) -> Result<(), String> {
    let entry = get_at(&history_path(), &id).ok_or("That entry no longer exists")?;
    crate::accessibility::paste::copy_to_clipboard(&entry.text).map_err(|e| e.to_string())
}

/// Bundle ids are reverse-DNS; refuse anything else before it reaches `open -b`.
fn valid_bundle_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id.len() < 256
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// "pasted" - sent Cmd+V into the original app; "copied" - only on the
/// clipboard (original app unknown/unreachable, or Accessibility missing).
#[tauri::command]
pub async fn paste_history_entry(app: AppHandle, id: String) -> Result<String, String> {
    let entry = get_at(&history_path(), &id).ok_or("That entry no longer exists")?;
    let own_bundle = app.config().identifier.clone();
    let target = entry
        .app_bundle_id
        .clone()
        .filter(|b| valid_bundle_id(b) && *b != own_bundle);

    // Always leave the text on the clipboard first: every failure below
    // degrades to "Copied - press Cmd+V".
    crate::accessibility::paste::copy_to_clipboard(&entry.text).map_err(|e| e.to_string())?;
    let Some(bundle) = target else {
        return Ok("copied".into());
    };
    if !crate::commands::permissions::accessibility_trusted() {
        return Ok("copied".into());
    }

    if let Some(win) = app.get_webview_window("main") {
        let _ = win.hide();
    }
    let text = entry.text;
    tokio::task::spawn_blocking(move || {
        // `open -b` raises the app without Apple Events.
        let activated = std::process::Command::new("open")
            .args(["-b", &bundle])
            .status()
            .is_ok_and(|s| s.success());
        if !activated {
            return Ok("copied".to_string());
        }
        // Let focus settle before the keystroke.
        std::thread::sleep(std::time::Duration::from_millis(350));
        match crate::accessibility::paste_text(&text) {
            Ok(()) => Ok("pasted".to_string()),
            Err(_) => Ok("copied".to_string()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u64, text: &str) -> HistoryEntry {
        HistoryEntry {
            id: format!("id{n}"),
            text: text.into(),
            timestamp_ms: n,
            duration_ms: 1000,
            app_name: Some("Notes".into()),
            app_bundle_id: Some("com.apple.Notes".into()),
            model: "ggml-base.en.bin".into(),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("b3t-history-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("history.jsonl")
    }

    #[test]
    fn append_then_load_newest_first() {
        let p = tmp("order");
        append_at(&p, &entry(1, "first"), 100).unwrap();
        append_at(&p, &entry(2, "second"), 100).unwrap();
        let all = load_at(&p);
        assert_eq!(all.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(get_at(&p, "id1").unwrap().text, "first");
        assert!(get_at(&p, "nope").is_none());
    }

    #[test]
    fn file_is_owner_only_even_when_it_pre_existed_loose() {
        let p = tmp("perms");
        append_at(&p, &entry(1, "secret"), 100).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&p), 0o600);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        append_at(&p, &entry(2, "more"), 100).unwrap();
        assert_eq!(mode(&p), 0o600);
        // Rewrites (trim/delete) keep it private too.
        delete_at(&p, "id1").unwrap();
        assert_eq!(mode(&p), 0o600);
    }

    #[test]
    fn cap_drops_the_oldest() {
        let p = tmp("cap");
        for n in 1..=15 {
            append_at(&p, &entry(n, &format!("t{n}")), 10).unwrap();
        }
        let all = load_at(&p);
        assert_eq!(all.len(), 10);
        assert_eq!(all.first().unwrap().text, "t15");
        assert_eq!(all.last().unwrap().text, "t6");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn delete_and_clear() {
        let p = tmp("del");
        append_at(&p, &entry(1, "a"), 100).unwrap();
        append_at(&p, &entry(2, "b"), 100).unwrap();
        assert!(delete_at(&p, "id1").unwrap());
        assert!(!delete_at(&p, "id1").unwrap());
        assert_eq!(load_at(&p).len(), 1);
        clear_at(&p).unwrap();
        assert!(load_at(&p).is_empty());
        assert!(!p.exists());
        clear_at(&p).unwrap(); // already gone: fine
    }

    #[test]
    fn corrupt_lines_are_skipped_not_fatal() {
        let p = tmp("corrupt");
        append_at(&p, &entry(1, "ok"), 100).unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"{not json\n\n").unwrap();
        drop(f);
        append_at(&p, &entry(2, "ok2"), 100).unwrap();
        assert_eq!(load_at(&p).len(), 2);
    }

    #[test]
    fn multiline_text_round_trips() {
        let p = tmp("multi");
        append_at(&p, &entry(1, "line one\nline \"two\"\t\u{1F600}"), 100).unwrap();
        assert_eq!(load_at(&p)[0].text, "line one\nline \"two\"\t\u{1F600}");
    }

    #[test]
    fn cap_is_clamped_and_ids_are_unique() {
        assert_eq!(clamp_cap(0), MIN_ENTRIES);
        assert_eq!(clamp_cap(1000), 1000);
        assert_eq!(clamp_cap(u32::MAX), MAX_ENTRIES);
        assert_ne!(new_id(5), new_id(5));
    }

    #[test]
    fn bundle_ids_are_validated() {
        assert!(valid_bundle_id("com.apple.Notes"));
        assert!(valid_bundle_id("com.microsoft.VSCode"));
        assert!(!valid_bundle_id(""));
        assert!(!valid_bundle_id("-a Calculator"));
        assert!(!valid_bundle_id("-evil"));
        assert!(!valid_bundle_id("com.x; rm -rf"));
    }
}
