import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CopyIcon, PageHeader, SearchIcon } from "./ui";

export interface HistoryEntry {
  id: string;
  text: string;
  timestamp_ms: number;
  duration_ms: number;
  app_name: string | null;
  app_bundle_id: string | null;
  model: string;
}

const PAGE = 100;

/** "just now", "5 min ago", "3 h ago", "yesterday", "4 d ago", else a date. */
export function relativeTime(ms: number, now: number = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 45) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.round(h / 24);
  if (d === 1) return "yesterday";
  if (d < 7) return `${d} d ago`;
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric", year: d > 300 ? "numeric" : undefined });
}

export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m ${s % 60}s`;
}

/** Case-insensitive match on the text or the app name. */
export function filterHistory(entries: HistoryEntry[], query: string): HistoryEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter((e) => e.text.toLowerCase().includes(q) || (e.app_name ?? "").toLowerCase().includes(q));
}

export default function HistoryPanel() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [enabled, setEnabled] = useState(true);
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(PAGE);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const load = useCallback(async () => {
    try {
      setEntries(await invoke<HistoryEntry[]>("get_history"));
    } catch (e) {
      setError(`Couldn't load history: ${e}`);
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    load();
    invoke<{ history_enabled?: boolean }>("get_settings")
      .then((s) => setEnabled(s?.history_enabled !== false))
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("history-updated", () => load()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  const shown = useMemo(() => filterHistory(entries, query), [entries, query]);

  const run = async (action: () => Promise<void>) => {
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    }
  };

  const copy = (entry: HistoryEntry) =>
    run(async () => {
      await invoke("copy_history_entry", { id: entry.id });
      setNotice("Copied");
    });

  const pasteAgain = (entry: HistoryEntry) =>
    run(async () => {
      const result = await invoke<string>("paste_history_entry", { id: entry.id });
      setNotice(result === "pasted" ? `Pasted into ${entry.app_name ?? "the app"}` : "Copied — press ⌘V");
    });

  const remove = (entry: HistoryEntry) =>
    run(async () => {
      await invoke("delete_history_entry", { id: entry.id });
      setEntries((prev) => prev.filter((e) => e.id !== entry.id));
    });

  const clearAll = () =>
    run(async () => {
      await invoke("clear_history");
      setEntries([]);
      setConfirmClear(false);
      setNotice("History cleared");
    });

  return (
    <div className="space-y-6">
      <PageHeader
        title="History"
        lede="Your recent dictations, kept only on this Mac. Turn it off or clear it in Settings."
        action={
          entries.length > 0 ? (
            confirmClear ? (
              <div className="flex shrink-0 gap-2">
                <button onClick={() => setConfirmClear(false)} className="btn-secondary">
                  Keep
                </button>
                <button onClick={clearAll} className="btn-danger-solid">
                  Delete all {entries.length}
                </button>
              </div>
            ) : (
              <button onClick={() => setConfirmClear(true)} className="btn-secondary shrink-0">
                Clear history
              </button>
            )
          ) : undefined
        }
      />

      {!enabled && (
        <p role="status" className="alert-danger">
          History is turned off, so new dictations aren&apos;t being saved. You can turn it on in Settings.
        </p>
      )}
      {notice && (
        <p role="status" className="px-1 text-xs text-ok">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="alert-danger">
          {error}
        </p>
      )}

      {entries.length > 0 && (
        <div className="relative">
          <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-subtle" />
          <input
            aria-label="Search history"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setLimit(PAGE);
            }}
            placeholder="Search text or app"
            className="input !pl-8"
          />
        </div>
      )}

      {loaded && entries.length === 0 && (
        <div className="card px-6 py-8 text-center">
          <p className="text-[13px] text-muted">No dictations yet. Finished ones will show up here.</p>
        </div>
      )}
      {entries.length > 0 && shown.length === 0 && (
        <div className="card px-6 py-8 text-center">
          <p className="text-[13px] text-muted">Nothing matches “{query}”.</p>
        </div>
      )}

      {shown.length > 0 && (
        <ul className="card card-list overflow-hidden">
          {shown.slice(0, limit).map((entry) => (
            <li key={entry.id} className="px-4 py-3">
              <p className="selectable line-clamp-4 whitespace-pre-wrap break-words text-[13px]">{entry.text}</p>
              <div className="mt-2 flex items-center justify-between gap-3">
                <p className="min-w-0 truncate text-xs text-subtle">
                  <span title={new Date(entry.timestamp_ms).toLocaleString()}>{relativeTime(entry.timestamp_ms)}</span>
                  {entry.app_name && <> · {entry.app_name}</>}
                  {entry.duration_ms > 0 && <> · {formatDuration(entry.duration_ms)}</>}
                </p>
                <div className="flex shrink-0 gap-1">
                  <button onClick={() => copy(entry)} aria-label="Copy dictation" className="btn-quiet inline-flex items-center gap-1">
                    <CopyIcon className="h-3.5 w-3.5" />
                    Copy
                  </button>
                  <button onClick={() => pasteAgain(entry)} aria-label="Paste dictation again" className="btn-quiet">
                    Paste again
                  </button>
                  <button onClick={() => remove(entry)} aria-label="Delete dictation" className="btn-danger">
                    Delete
                  </button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {shown.length > limit && (
        <div className="text-center">
          <button onClick={() => setLimit((l) => l + PAGE)} className="btn-secondary">
            Show more ({shown.length - limit})
          </button>
        </div>
      )}
    </div>
  );
}
