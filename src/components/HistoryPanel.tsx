import { useCallback, useEffect, useRef, useState } from "react";
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
  words?: number;
  source?: string;
}

interface HistoryPage {
  entries: HistoryEntry[];
  total: number;
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

export default function HistoryPanel() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [everything, setEverything] = useState(0);
  const [loaded, setLoaded] = useState(false);
  const [enabled, setEnabled] = useState(true);
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [fluidVoice, setFluidVoice] = useState(false);
  const [importing, setImporting] = useState(false);
  const queryRef = useRef(query);
  queryRef.current = query;

  /** Reload the first `count` matches for the current search. */
  const load = useCallback(async (count = PAGE) => {
    try {
      const q = queryRef.current;
      const page = await invoke<HistoryPage>("get_history", { query: q, limit: count, offset: 0 });
      if (q !== queryRef.current) return; // a newer search is on its way
      setEntries(page?.entries ?? []);
      setTotal(page?.total ?? 0);
      if (!q.trim()) setEverything(page?.total ?? 0);
    } catch (e) {
      setError(`Couldn't load history: ${e}`);
    } finally {
      setLoaded(true);
    }
  }, []);

  const loadRef = useRef(load);
  const shownRef = useRef(PAGE);
  shownRef.current = Math.min(500, Math.max(PAGE, entries.length));

  useEffect(() => {
    loadRef.current();
    invoke<{ history_enabled?: boolean }>("get_settings")
      .then((s) => setEnabled(s?.history_enabled !== false))
      .catch(() => {});
    invoke<boolean>("fluidvoice_history_available")
      .then((v) => setFluidVoice(!!v))
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("history-updated", () => loadRef.current(shownRef.current)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // Search in the database, a moment after typing stops.
  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => load(PAGE), 180);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const showMore = async () => {
    try {
      const page = await invoke<HistoryPage>("get_history", { query, limit: PAGE, offset: entries.length });
      setEntries((prev) => [...prev, ...(page?.entries ?? [])]);
      setTotal(page?.total ?? total);
    } catch (e) {
      setError(`Couldn't load history: ${e}`);
    }
  };

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
      setTotal((t) => Math.max(0, t - 1));
      setEverything((t) => Math.max(0, t - 1));
    });

  const clearAll = () =>
    run(async () => {
      await invoke("clear_history");
      setEntries([]);
      setTotal(0);
      setEverything(0);
      setConfirmClear(false);
      setNotice("History cleared");
    });

  const importFluidVoice = () =>
    run(async () => {
      setImporting(true);
      try {
        const r = await invoke<{ imported: number }>("import_fluidvoice_history");
        setNotice(r.imported ? `Brought over ${r.imported.toLocaleString()} dictations from FluidVoice` : "Already up to date with FluidVoice");
        await load(PAGE);
      } finally {
        setImporting(false);
      }
    });

  return (
    <div className="space-y-6">
      <PageHeader
        title="History"
        lede="Every dictation, kept only on this Mac. Turn it off, limit it or clear it in Settings."
        action={
          everything > 0 ? (
            confirmClear ? (
              <div className="flex shrink-0 gap-2">
                <button onClick={() => setConfirmClear(false)} className="btn-secondary">
                  Keep
                </button>
                <button onClick={clearAll} className="btn-danger-solid">
                  Delete all {everything.toLocaleString()}
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

      {fluidVoice && (
        <div className="card flex items-center justify-between gap-3 px-4 py-3">
          <p className="text-xs text-muted">Bring over your FluidVoice history. Already imported entries are skipped; window titles are left behind.</p>
          <button onClick={importFluidVoice} disabled={importing} className="btn-secondary shrink-0">
            {importing ? "Importing…" : "Import from FluidVoice"}
          </button>
        </div>
      )}

      {everything > 0 && (
        <div className="relative">
          <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-subtle" />
          <input
            aria-label="Search history"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`Search ${everything.toLocaleString()} dictations by text or app`}
            className="input !pl-8"
          />
        </div>
      )}

      {loaded && everything === 0 && (
        <div className="card px-6 py-8 text-center">
          <p className="text-[13px] text-muted">No dictations yet. Finished ones will show up here.</p>
        </div>
      )}
      {loaded && everything > 0 && total === 0 && query.trim() !== "" && (
        <div className="card px-6 py-8 text-center">
          <p className="text-[13px] text-muted">Nothing matches “{query}”.</p>
        </div>
      )}

      {entries.length > 0 && (
        <ul className="card card-list overflow-hidden">
          {entries.map((entry) => (
            <li key={entry.id} className="px-4 py-3">
              <p className="selectable line-clamp-4 whitespace-pre-wrap break-words text-[13px]">{entry.text}</p>
              <div className="mt-2 flex items-center justify-between gap-3">
                <p className="min-w-0 truncate text-xs text-subtle">
                  <span title={new Date(entry.timestamp_ms).toLocaleString()}>{relativeTime(entry.timestamp_ms)}</span>
                  {entry.app_name && <> · {entry.app_name}</>}
                  {entry.source === "fluidvoice" && <> · FluidVoice</>}
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
      {total > entries.length && (
        <div className="text-center">
          <button onClick={showMore} className="btn-secondary">
            Show more ({(total - entries.length).toLocaleString()})
          </button>
        </div>
      )}
    </div>
  );
}
