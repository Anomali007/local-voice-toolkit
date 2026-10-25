//! Local dictation history in SQLite at
//! `<app data>/com.blahcubed.app/history.sqlite3`, kept forever by default
//! (Settings can cap it), plus the numbers behind the Stats page and an
//! importer for FluidVoice's history.
//!
//! Privacy: the database holds transcripts, so it and its WAL/SHM files are
//! owner-only (0600), it never leaves the machine, and text is NEVER written
//! to the log (only counts). Clearing deletes the rows and vacuums, so the
//! text does not linger in free pages.

use chrono::{Duration as Days, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

use crate::accessibility::FrontmostAppInfo;
use crate::commands::settings::AppSettings;

const MIN_ENTRIES: usize = 10;
const MAX_ENTRIES: usize = 1_000_000;
const PAGE_MAX: u32 = 500;
/// Seconds from 1970-01-01 to 2001-01-01 (Apple's reference date).
const APPLE_EPOCH_OFFSET: f64 = 978_307_200.0;

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
    #[serde(default)]
    pub words: u32,
    /// "blah3", or "fluidvoice" for imported entries.
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "blah3".into()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HistoryPage {
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
    /// Matching entries in total (for "Show more").
    pub total: u64,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS entries (
    id            TEXT PRIMARY KEY,
    timestamp_ms  INTEGER NOT NULL,
    text          TEXT NOT NULL,
    duration_ms   INTEGER NOT NULL DEFAULT 0,
    app_name      TEXT,
    app_bundle_id TEXT,
    model         TEXT NOT NULL DEFAULT '',
    words         INTEGER NOT NULL DEFAULT 0,
    source        TEXT NOT NULL DEFAULT 'blah3'
);
CREATE INDEX IF NOT EXISTS entries_time ON entries(timestamp_ms);
";

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn history_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.blahcubed.app")
        .join("history.sqlite3")
}

/// The JSON-lines file older builds wrote, next to the database.
fn legacy_path(db: &Path) -> PathBuf {
    db.with_file_name("history.jsonl")
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

pub fn count_words(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}

/// The Settings value as a cap: 0 keeps everything.
pub fn cap_for(keep: u32) -> Option<usize> {
    (keep != 0).then(|| (keep as usize).clamp(MIN_ENTRIES, MAX_ENTRIES))
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

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// The database and the WAL/SHM files SQLite keeps beside it.
fn db_files(path: &Path) -> [PathBuf; 3] {
    [path.to_path_buf(), with_suffix(path, "-wal"), with_suffix(path, "-shm")]
}

fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Create the file owner-only before SQLite does; its WAL and SHM files
    // take their mode from it.
    if !path.exists() {
        let _ = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(path);
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
    conn.execute_batch(SCHEMA)?;
    for f in db_files(path) {
        ensure_private(&f);
    }
    migrate_legacy(&conn, path);
    Ok(conn)
}

/// Move entries from the old JSON-lines file into the database, then delete
/// the file. Safe to repeat: ids are kept, so nothing is doubled.
fn migrate_legacy(conn: &Connection, db: &Path) {
    let legacy = legacy_path(db);
    let Ok(content) = std::fs::read_to_string(&legacy) else { return };
    let mut entries: Vec<HistoryEntry> = content
        .lines()
        .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        .collect();
    for e in &mut entries {
        e.words = count_words(&e.text);
    }
    match insert_all(conn, &entries) {
        Ok(_) => {
            let _ = std::fs::remove_file(&legacy);
            tracing::info!("History: moved {} entries from the old file", entries.len());
        }
        Err(e) => tracing::warn!("History: could not move the old file yet: {}", e),
    }
}

/// Insert, keeping any entry with the same id. Returns how many were new.
fn insert_all(conn: &Connection, entries: &[HistoryEntry]) -> rusqlite::Result<u64> {
    let tx = conn.unchecked_transaction()?;
    let mut added = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO entries
             (id, timestamp_ms, text, duration_ms, app_name, app_bundle_id, model, words, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for e in entries {
            added += stmt.execute(params![
                e.id,
                e.timestamp_ms as i64,
                e.text,
                e.duration_ms as i64,
                e.app_name,
                e.app_bundle_id,
                e.model,
                e.words,
                e.source,
            ])? as u64;
        }
    }
    tx.commit()?;
    Ok(added)
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        timestamp_ms: row.get::<_, i64>(1)?.max(0) as u64,
        text: row.get(2)?,
        duration_ms: row.get::<_, i64>(3)?.max(0) as u64,
        app_name: row.get(4)?,
        app_bundle_id: row.get(5)?,
        model: row.get(6)?,
        words: row.get(7)?,
        source: row.get(8)?,
    })
}

const COLUMNS: &str = "id, timestamp_ms, text, duration_ms, app_name, app_bundle_id, model, words, source";

/// Add `entry`, then drop the oldest entries beyond `cap` (None keeps all).
pub fn append_at(path: &Path, entry: &HistoryEntry, cap: Option<usize>) -> rusqlite::Result<()> {
    let conn = open(path)?;
    insert_all(&conn, std::slice::from_ref(entry))?;
    if let Some(cap) = cap {
        conn.execute(
            "DELETE FROM entries WHERE id IN
             (SELECT id FROM entries ORDER BY timestamp_ms DESC, rowid DESC LIMIT -1 OFFSET ?1)",
            [cap as i64],
        )?;
    }
    Ok(())
}

fn escape_like(q: &str) -> String {
    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// One page, newest first, matching `query` in the text or app name
/// (case-insensitive for ASCII).
pub fn query_at(path: &Path, query: &str, limit: u32, offset: u32) -> rusqlite::Result<HistoryPage> {
    let conn = open(path)?;
    let q = query.trim();
    let filter = if q.is_empty() {
        ""
    } else {
        "WHERE text LIKE ?3 ESCAPE '\\' OR app_name LIKE ?3 ESCAPE '\\'"
    };
    let pattern = format!("%{}%", escape_like(q));
    let limit = limit.clamp(1, PAGE_MAX) as i64;

    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM entries {filter} ORDER BY timestamp_ms DESC, rowid DESC LIMIT ?1 OFFSET ?2"
    ))?;
    let entries = if q.is_empty() {
        stmt.query_map(params![limit, offset as i64], row_to_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![limit, offset as i64, pattern], row_to_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let total: i64 = if q.is_empty() {
        conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE text LIKE ?1 ESCAPE '\\' OR app_name LIKE ?1 ESCAPE '\\'",
            [pattern],
            |r| r.get(0),
        )?
    };
    Ok(HistoryPage { entries, total: total as u64 })
}

pub fn get_at(path: &Path, id: &str) -> Option<HistoryEntry> {
    let conn = open(path).ok()?;
    conn.query_row(&format!("SELECT {COLUMNS} FROM entries WHERE id = ?1"), [id], row_to_entry)
        .optional()
        .ok()
        .flatten()
}

/// Returns whether anything was removed.
pub fn delete_at(path: &Path, id: &str) -> rusqlite::Result<bool> {
    let conn = open(path)?;
    Ok(conn.execute("DELETE FROM entries WHERE id = ?1", [id])? > 0)
}

/// Delete every entry and scrub the text from the files on disk.
pub fn clear_at(path: &Path) -> rusqlite::Result<()> {
    let conn = open(path)?;
    conn.execute("DELETE FROM entries", [])?;
    conn.execute_batch("VACUUM;")?;
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    let _ = std::fs::remove_file(legacy_path(path));
    Ok(())
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
        words: count_words(text),
        source: default_source(),
    };
    match append_at(&history_path(), &entry, cap_for(settings.history_keep)) {
        Ok(()) => {
            tracing::debug!("History: saved entry ({} words)", entry.words);
            let _ = app.emit("history-updated", ());
        }
        Err(e) => tracing::warn!("History: could not save entry: {}", e),
    }
}

// ---------------------------------------------------------------- FluidVoice

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ImportResult {
    pub imported: u64,
    pub already_here: u64,
    pub skipped: u64,
}

pub fn fluidvoice_history_file() -> Option<PathBuf> {
    let path = dirs::data_dir()?.join("FluidVoice").join("TranscriptionHistory.sqlite3");
    path.is_file().then_some(path)
}

/// The fields we take from a FluidVoice entry. Its window title is
/// deliberately not read: Blah³ does not keep window titles.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FluidVoiceEntry {
    id: String,
    timestamp: f64,
    app_name: Option<String>,
    raw_text: Option<String>,
    processed_text: Option<String>,
}

impl FluidVoiceEntry {
    fn into_entry(self) -> Option<HistoryEntry> {
        let text = [self.processed_text, self.raw_text]
            .into_iter()
            .flatten()
            .find(|t| !t.trim().is_empty())?;
        // FluidVoice stores seconds since 2001 (Apple's reference date).
        let unix = if self.timestamp < 1.2e9 { self.timestamp + APPLE_EPOCH_OFFSET } else { self.timestamp };
        if !unix.is_finite() || unix <= 0.0 {
            return None;
        }
        Some(HistoryEntry {
            id: format!("fv-{}", self.id),
            words: count_words(&text),
            text,
            timestamp_ms: (unix * 1000.0).round() as u64,
            duration_ms: 0,
            app_name: self.app_name.filter(|n| !n.is_empty()),
            app_bundle_id: None,
            model: "fluidvoice".into(),
            source: "fluidvoice".into(),
        })
    }
}

/// Removes a private scratch directory however the import ends.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copy FluidVoice's database (with its WAL, which holds the newest entries)
/// into a private scratch folder and read it there, so FluidVoice's own files
/// are never opened for writing. Re-importing adds only what is new.
pub fn import_fluidvoice_at(dest: &Path, source: &Path) -> Result<ImportResult, String> {
    let parent = dest.parent().ok_or("No folder for history")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let scratch = ScratchDir(parent.join(format!(".fluidvoice-import-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&scratch.0);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&scratch.0)
        .map_err(|e| format!("Could not make a private scratch folder: {e}"))?;

    let copy = scratch.0.join("history.sqlite3");
    for (from, to) in db_files(source).iter().zip(db_files(&copy).iter()) {
        if from.exists() {
            std::fs::copy(from, to).map_err(|e| format!("Could not read FluidVoice's history: {e}"))?;
            let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o600));
        }
    }

    let src = Connection::open(&copy).map_err(|e| format!("FluidVoice's history could not be opened: {e}"))?;
    let mut stmt = src
        .prepare("SELECT payload FROM history")
        .map_err(|e| format!("This is not a FluidVoice history file: {e}"))?;
    let mut skipped = 0u64;
    let mut entries = Vec::new();
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let parsed = row
            .get_ref(0)
            .ok()
            .and_then(|v| v.as_bytes().ok())
            .and_then(|b| serde_json::from_slice::<FluidVoiceEntry>(b).ok())
            .and_then(FluidVoiceEntry::into_entry);
        match parsed {
            Some(e) => entries.push(e),
            None => skipped += 1,
        }
    }

    let conn = open(dest).map_err(|e| e.to_string())?;
    let imported = insert_all(&conn, &entries).map_err(|e| e.to_string())?;
    Ok(ImportResult { imported, already_here: entries.len() as u64 - imported, skipped })
}

// --------------------------------------------------------------------- Stats

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DayStat {
    /// Local date, "YYYY-MM-DD".
    pub date: String,
    pub words: u64,
    pub dictations: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AppStat {
    pub name: String,
    pub dictations: u32,
    pub words: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Stats {
    pub today: String,
    pub total_words: u64,
    pub total_dictations: u64,
    /// Characters dictated: the keystrokes saved.
    pub total_chars: u64,
    pub active_days: u32,
    /// Consecutive days with a dictation, ending today (or yesterday when
    /// today has none yet).
    pub current_streak: u32,
    pub best_streak: u32,
    pub today_words: u64,
    pub today_dictations: u32,
    /// Every day with a dictation, oldest first.
    pub days: Vec<DayStat>,
    /// Dictations started in each local hour, 0-23.
    pub hours: Vec<u32>,
    /// Most-used apps first (top 8).
    pub apps: Vec<AppStat>,
    pub longest_dictation_words: u32,
    pub best_day: Option<DayStat>,
    pub busiest_day: Option<DayStat>,
    pub imported_from_fluidvoice: u64,
}

pub struct StatRow {
    pub at: NaiveDateTime,
    pub words: u32,
    pub chars: u32,
    pub app: Option<String>,
    pub fluidvoice: bool,
}

pub fn compute_stats(rows: &[StatRow], today: NaiveDate) -> Stats {
    let mut by_day: BTreeMap<NaiveDate, (u64, u32)> = BTreeMap::new();
    let mut hours = vec![0u32; 24];
    let mut apps: HashMap<&str, (u32, u64)> = HashMap::new();
    let (mut words, mut chars, mut longest, mut imported) = (0u64, 0u64, 0u32, 0u64);

    for r in rows {
        let day = by_day.entry(r.at.date()).or_default();
        day.0 += r.words as u64;
        day.1 += 1;
        hours[r.at.hour() as usize] += 1;
        let app = apps.entry(r.app.as_deref().unwrap_or("Unknown")).or_default();
        app.0 += 1;
        app.1 += r.words as u64;
        words += r.words as u64;
        chars += r.chars as u64;
        longest = longest.max(r.words);
        imported += r.fluidvoice as u64;
    }

    let mut best_streak = 0u32;
    let mut run = 0u32;
    let mut prev: Option<NaiveDate> = None;
    for &d in by_day.keys() {
        run = if prev.is_some_and(|p| d - p == Days::days(1)) { run + 1 } else { 1 };
        best_streak = best_streak.max(run);
        prev = Some(d);
    }
    let mut cursor = if by_day.contains_key(&today) { today } else { today - Days::days(1) };
    let mut current_streak = 0u32;
    while by_day.contains_key(&cursor) {
        current_streak += 1;
        cursor -= Days::days(1);
    }

    let days: Vec<DayStat> = by_day
        .iter()
        .map(|(d, &(w, n))| DayStat { date: d.format("%Y-%m-%d").to_string(), words: w, dictations: n })
        .collect();
    // Earliest day wins a tie, so the record does not move to a later equal day.
    let best_day = days.iter().fold(None::<&DayStat>, |b, d| match b {
        Some(b) if b.words >= d.words => Some(b),
        _ => Some(d),
    });
    let busiest_day = days.iter().fold(None::<&DayStat>, |b, d| match b {
        Some(b) if b.dictations >= d.dictations => Some(b),
        _ => Some(d),
    });
    let (today_words, today_dictations) = by_day.get(&today).copied().unwrap_or_default();

    let mut apps: Vec<AppStat> = apps
        .into_iter()
        .map(|(name, (n, w))| AppStat { name: name.to_string(), dictations: n, words: w })
        .collect();
    apps.sort_by(|a, b| {
        (b.dictations, b.words).cmp(&(a.dictations, a.words)).then_with(|| a.name.cmp(&b.name))
    });
    apps.truncate(8);

    Stats {
        today: today.format("%Y-%m-%d").to_string(),
        total_words: words,
        total_dictations: rows.len() as u64,
        total_chars: chars,
        active_days: by_day.len() as u32,
        current_streak,
        best_streak,
        today_words,
        today_dictations,
        best_day: best_day.cloned(),
        busiest_day: busiest_day.cloned(),
        days,
        hours,
        apps,
        longest_dictation_words: longest,
        imported_from_fluidvoice: imported,
    }
}

/// Stats with days in this Mac's time zone.
pub fn stats_at(path: &Path) -> rusqlite::Result<Stats> {
    let conn = open(path)?;
    let mut stmt = conn.prepare("SELECT timestamp_ms, words, length(text), app_name, source FROM entries")?;
    let rows = stmt
        .query_map([], |r| {
            let ms: i64 = r.get(0)?;
            Ok(Local.timestamp_millis_opt(ms).single().map(|at| StatRow {
                at: at.naive_local(),
                words: r.get(1).unwrap_or(0),
                chars: r.get(2).unwrap_or(0),
                app: r.get(3).unwrap_or(None),
                fluidvoice: r.get::<_, String>(4).is_ok_and(|s| s == "fluidvoice"),
            }))
        })?
        .filter_map(|r| r.ok().flatten())
        .collect::<Vec<_>>();
    Ok(compute_stats(&rows, Local::now().date_naive()))
}

// ------------------------------------------------------------------ Commands

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_history(query: Option<String>, limit: Option<u32>, offset: Option<u32>) -> Result<HistoryPage, String> {
    blocking(move || {
        query_at(&history_path(), query.as_deref().unwrap_or(""), limit.unwrap_or(100), offset.unwrap_or(0))
            .map_err(|e| format!("Could not read history: {e}"))
    })
    .await
}

#[tauri::command]
pub async fn get_history_stats() -> Result<Stats, String> {
    blocking(|| stats_at(&history_path()).map_err(|e| format!("Could not read history: {e}"))).await
}

#[tauri::command]
pub fn fluidvoice_history_available() -> bool {
    fluidvoice_history_file().is_some()
}

#[tauri::command]
pub async fn import_fluidvoice_history(app: AppHandle) -> Result<ImportResult, String> {
    let source = fluidvoice_history_file().ok_or("FluidVoice's history wasn't found on this Mac")?;
    let result = blocking(move || import_fluidvoice_at(&history_path(), &source)).await?;
    tracing::info!(
        "History: FluidVoice import added {}, {} already here, {} skipped",
        result.imported,
        result.already_here,
        result.skipped
    );
    let _ = app.emit("history-updated", ());
    Ok(result)
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
            words: count_words(text),
            source: "blah3".into(),
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("b3t-history-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("history.sqlite3")
    }

    fn all(p: &Path) -> Vec<HistoryEntry> {
        query_at(p, "", PAGE_MAX, 0).unwrap().entries
    }

    fn file_bytes(p: &Path) -> Vec<u8> {
        db_files(p).iter().flat_map(|f| std::fs::read(f).unwrap_or_default()).collect()
    }

    fn contains(hay: &[u8], needle: &str) -> bool {
        hay.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    #[test]
    fn append_then_load_newest_first() {
        let p = tmp("order");
        append_at(&p, &entry(1, "first"), None).unwrap();
        append_at(&p, &entry(2, "second one"), None).unwrap();
        let got = all(&p);
        assert_eq!(got.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["second one", "first"]);
        assert_eq!(got[0].words, 2);
        assert_eq!(get_at(&p, "id1").unwrap().text, "first");
        assert!(get_at(&p, "nope").is_none());
    }

    #[test]
    fn files_are_owner_only_even_when_they_pre_existed_loose() {
        let p = tmp("perms");
        append_at(&p, &entry(1, "secret"), None).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        for f in db_files(&p).iter().filter(|f| f.exists()) {
            assert_eq!(mode(f), 0o600, "{}", f.display());
        }
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        append_at(&p, &entry(2, "more"), None).unwrap();
        assert_eq!(mode(&p), 0o600);
    }

    #[test]
    fn keeps_everything_by_default_and_a_cap_drops_the_oldest() {
        let p = tmp("cap");
        for n in 1..=30 {
            append_at(&p, &entry(n, &format!("t{n}")), cap_for(0)).unwrap();
        }
        assert_eq!(query_at(&p, "", 1, 0).unwrap().total, 30);
        append_at(&p, &entry(31, "t31"), cap_for(10)).unwrap();
        let got = all(&p);
        assert_eq!(got.len(), 10);
        assert_eq!(got.first().unwrap().text, "t31");
        assert_eq!(got.last().unwrap().text, "t22");
    }

    #[test]
    fn cap_values() {
        assert_eq!(cap_for(0), None);
        assert_eq!(cap_for(3), Some(MIN_ENTRIES));
        assert_eq!(cap_for(5000), Some(5000));
        assert_ne!(new_id(5), new_id(5));
    }

    #[test]
    fn search_and_paging() {
        let p = tmp("search");
        for n in 1..=12 {
            append_at(&p, &entry(n, &format!("note {n} about 50% of_it")), None).unwrap();
        }
        append_at(&p, &HistoryEntry { app_name: Some("Ghostty".into()), ..entry(13, "other") }, None).unwrap();
        let page = query_at(&p, "NOTE", 5, 5).unwrap();
        assert_eq!(page.total, 12);
        assert_eq!(page.entries.len(), 5);
        assert_eq!(page.entries[0].text, "note 7 about 50% of_it");
        assert_eq!(query_at(&p, "ghostty", 10, 0).unwrap().total, 1);
        // LIKE wildcards in the query are literal.
        assert_eq!(query_at(&p, "50%", 50, 0).unwrap().total, 12);
        assert_eq!(query_at(&p, "e_1", 50, 0).unwrap().total, 0, "_ must not match the space in \"note 1\"");
        assert_eq!(query_at(&p, "zzz", 50, 0).unwrap().total, 0);
    }

    #[test]
    fn delete_and_clear_scrub_the_text_from_disk() {
        let p = tmp("del");
        append_at(&p, &entry(1, "a"), None).unwrap();
        append_at(&p, &entry(2, "CANARY-PHRASE-42"), None).unwrap();
        assert!(delete_at(&p, "id1").unwrap());
        assert!(!delete_at(&p, "id1").unwrap());
        assert_eq!(all(&p).len(), 1);
        assert!(contains(&file_bytes(&p), "CANARY-PHRASE-42"), "control: the text is on disk before clearing");
        clear_at(&p).unwrap();
        assert!(all(&p).is_empty());
        assert!(!contains(&file_bytes(&p), "CANARY-PHRASE-42"));
    }

    #[test]
    fn multiline_text_round_trips() {
        let p = tmp("multi");
        append_at(&p, &entry(1, "line one\nline \"two\"\t\u{1F600}"), None).unwrap();
        assert_eq!(all(&p)[0].text, "line one\nline \"two\"\t\u{1F600}");
    }

    #[test]
    fn old_json_lines_file_is_moved_in_once() {
        let p = tmp("legacy");
        let legacy = legacy_path(&p);
        let old = r#"{"id":"a","text":"hello there","timestamp_ms":5,"duration_ms":0,"app_name":null,"app_bundle_id":null,"model":"m"}"#;
        std::fs::write(&legacy, format!("{old}\n{{broken\n")).unwrap();
        let got = all(&p);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].words, got[0].source.as_str()), (2, "blah3"));
        assert!(!legacy.exists());
        assert_eq!(all(&p).len(), 1);
    }

    fn fake_fluidvoice(dir: &Path, payloads: &[String]) -> PathBuf {
        let path = dir.join("TranscriptionHistory.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; CREATE TABLE history (id TEXT PRIMARY KEY, payload BLOB NOT NULL);
             CREATE TABLE metadata (key TEXT PRIMARY KEY);",
        )
        .unwrap();
        for (i, p) in payloads.iter().enumerate() {
            conn.execute("INSERT INTO history VALUES (?1, ?2)", params![i.to_string(), p.as_bytes()]).unwrap();
        }
        // Leave the connection open-ish: entries can still sit in the WAL.
        std::mem::forget(conn);
        path
    }

    fn fv(id: &str, ts: f64, text: &str) -> String {
        serde_json::json!({
            "id": id, "timestamp": ts, "appName": "Ghostty", "windowTitle": "SECRET-WINDOW-TITLE",
            "rawText": text, "processedText": text, "characterCount": text.len(),
            "isStarred": false, "wasAIProcessed": false
        })
        .to_string()
    }

    #[test]
    fn fluidvoice_import_maps_fields_skips_junk_and_is_idempotent() {
        let dest = tmp("fv-dest");
        let src_dir = dest.parent().unwrap().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        // 2026-10-24 03:28:41 UTC as seconds since 2001.
        let src = fake_fluidvoice(
            &src_dir,
            &[fv("A", 814_505_321.0, "three little words"), fv("B", 814_505_400.0, "   "), "not json".into()],
        );
        let r = import_fluidvoice_at(&dest, &src).unwrap();
        assert_eq!(r, ImportResult { imported: 1, already_here: 0, skipped: 2 });
        let e = &all(&dest)[0];
        assert_eq!(e.id, "fv-A");
        assert_eq!(e.timestamp_ms, 1_792_812_521_000);
        assert_eq!((e.words, e.app_name.as_deref(), e.source.as_str()), (3, Some("Ghostty"), "fluidvoice"));
        assert!(!contains(&file_bytes(&dest), "SECRET-WINDOW-TITLE"));
        let again = import_fluidvoice_at(&dest, &src).unwrap();
        assert_eq!((again.imported, again.already_here), (0, 1));
        // The scratch copy is gone.
        let leftovers = std::fs::read_dir(dest.parent().unwrap())
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(".fluidvoice-import"))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn importing_something_else_fails_cleanly() {
        let dest = tmp("fv-bad");
        let bogus = dest.with_file_name("bogus.sqlite3");
        std::fs::write(&bogus, b"definitely not sqlite").unwrap();
        assert!(import_fluidvoice_at(&dest, &bogus).is_err());
    }

    fn at(day: &str, hour: u32, words: u32, app: &str) -> StatRow {
        StatRow {
            at: NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap().and_hms_opt(hour, 0, 0).unwrap(),
            words,
            chars: words * 5,
            app: Some(app.into()),
            fluidvoice: app == "Ghostty",
        }
    }

    #[test]
    fn stats_streaks_bests_hours_and_apps() {
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let rows = vec![
            at("2026-01-01", 9, 10, "Notes"),
            at("2026-01-02", 9, 10, "Notes"),
            at("2026-01-03", 9, 10, "Notes"),
            // gap on the 4th
            at("2026-01-05", 0, 500, "Ghostty"),
            at("2026-01-06", 0, 7, "Ghostty"),
            at("2026-01-06", 23, 8, "Ghostty"),
            at("2026-01-06", 23, 9, "Slack"),
        ];
        let s = compute_stats(&rows, d("2026-01-07"));
        assert_eq!((s.total_words, s.total_dictations, s.total_chars), (554, 7, 2770));
        assert_eq!((s.active_days, s.best_streak), (5, 3));
        assert_eq!(s.current_streak, 2, "today has none yet, so the streak runs to yesterday");
        assert_eq!(compute_stats(&rows, d("2026-01-06")).current_streak, 2);
        assert_eq!(compute_stats(&rows, d("2026-01-08")).current_streak, 0);
        assert_eq!((s.today_words, s.today_dictations), (0, 0));
        assert_eq!(s.longest_dictation_words, 500);
        assert_eq!(s.best_day.as_ref().unwrap().date, "2026-01-05");
        assert_eq!(s.busiest_day.as_ref().unwrap(), &DayStat { date: "2026-01-06".into(), words: 24, dictations: 3 });
        assert_eq!((s.hours[0], s.hours[9], s.hours[23]), (2, 3, 2));
        // Notes and Ghostty tie on dictations; more words ranks first.
        assert_eq!(s.apps[0], AppStat { name: "Ghostty".into(), dictations: 3, words: 515 });
        assert_eq!(s.apps[1], AppStat { name: "Notes".into(), dictations: 3, words: 30 });
        assert_eq!(s.imported_from_fluidvoice, 3);
        assert_eq!(s.days.first().unwrap().date, "2026-01-01");
    }

    #[test]
    fn empty_history_has_zeroed_stats() {
        let s = compute_stats(&[], NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!((s.total_words, s.current_streak, s.best_streak), (0, 0, 0));
        assert!(s.best_day.is_none() && s.apps.is_empty() && s.days.is_empty());
        assert_eq!(s.hours.len(), 24);
    }

    /// Imports this Mac's real FluidVoice history into a scratch database and
    /// prints only totals. `cargo test real_fluidvoice -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_fluidvoice_import_and_stats() {
        let Some(src) = fluidvoice_history_file() else { return };
        let dest = tmp("real-fv");
        let r = import_fluidvoice_at(&dest, &src).unwrap();
        let s = stats_at(&dest).unwrap();
        println!(
            "imported={} skipped={} words={} dictations={} chars={} active_days={} streak={}/{} longest={} best_day={:?} busiest={:?} top_app_counts={:?} peak_hour={:?}",
            r.imported, r.skipped, s.total_words, s.total_dictations, s.total_chars, s.active_days,
            s.current_streak, s.best_streak, s.longest_dictation_words,
            s.best_day.as_ref().map(|d| d.words), s.busiest_day.as_ref().map(|d| d.dictations),
            s.apps.iter().take(5).map(|a| a.dictations).collect::<Vec<_>>(),
            s.hours.iter().enumerate().max_by_key(|(_, n)| **n).map(|(h, _)| h),
        );
        // Totals only (no text), for previewing the Stats page with real numbers.
        if let Ok(out) = std::env::var("B3_STATS_JSON") {
            std::fs::write(out, serde_json::to_string(&s).unwrap()).unwrap();
        }
        let _ = std::fs::remove_dir_all(dest.parent().unwrap());
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
