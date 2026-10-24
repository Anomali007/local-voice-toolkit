import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface DictionaryEntry {
  id: string;
  triggers: string[];
  replacement: string;
}

/** "mass lab, the mass lab" -> ["mass lab", "the mass lab"] */
export function parseTriggers(input: string): string[] {
  return input
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
}

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;

export default function DictionaryPanel() {
  const [entries, setEntries] = useState<DictionaryEntry[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [replacement, setReplacement] = useState("");
  const [triggers, setTriggers] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [tryText, setTryText] = useState("");
  const [tryResult, setTryResult] = useState("");
  const [fluidAvailable, setFluidAvailable] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<DictionaryEntry[]>("get_dictionary")
      .then(setEntries)
      .catch((e) => setError(String(e)))
      .finally(() => setLoaded(true));
    invoke<boolean>("fluidvoice_dictionary_available")
      .then(setFluidAvailable)
      .catch(() => setFluidAvailable(false));
  }, []);

  // Live preview of the dictionary on a sample sentence.
  useEffect(() => {
    if (!tryText.trim()) {
      setTryResult("");
      return;
    }
    const handle = setTimeout(() => {
      invoke<string>("preview_dictionary", { text: tryText, entries })
        .then(setTryResult)
        .catch(() => setTryResult(""));
    }, 150);
    return () => clearTimeout(handle);
  }, [tryText, entries]);

  const persist = async (next: DictionaryEntry[]) => {
    setError(null);
    try {
      setEntries(await invoke<DictionaryEntry[]>("save_dictionary", { entries: next }));
    } catch (e) {
      setError(`Couldn't save the dictionary: ${e}`);
    }
  };

  const resetForm = () => {
    setReplacement("");
    setTriggers("");
    setEditingId(null);
  };

  const submit = async () => {
    const parsed = parseTriggers(triggers);
    if (!replacement.trim() || parsed.length === 0) return;
    const entry = { id: editingId ?? newId(), triggers: parsed, replacement: replacement.trim() };
    const next = editingId
      ? entries.map((e) => (e.id === editingId ? entry : e))
      : [...entries, entry];
    await persist(next);
    setNotice(editingId ? `Updated “${entry.replacement}”` : `Added “${entry.replacement}”`);
    resetForm();
  };

  const edit = (entry: DictionaryEntry) => {
    setEditingId(entry.id);
    setReplacement(entry.replacement);
    setTriggers(entry.triggers.join(", "));
  };

  const remove = async (id: string) => {
    await persist(entries.filter((e) => e.id !== id));
    if (editingId === id) resetForm();
  };

  const importFluid = async () => {
    setError(null);
    try {
      const [merged, added] = await invoke<[DictionaryEntry[], number]>("import_fluidvoice_dictionary");
      setEntries(merged);
      setNotice(added ? `Imported ${added} ${added === 1 ? "entry" : "entries"} from FluidVoice` : "Already up to date with FluidVoice");
    } catch (e) {
      setError(`Import failed: ${e}`);
    }
  };

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    const sorted = [...entries].sort((a, b) => a.replacement.localeCompare(b.replacement));
    if (!q) return sorted;
    return sorted.filter(
      (e) => e.replacement.toLowerCase().includes(q) || e.triggers.some((t) => t.toLowerCase().includes(q)),
    );
  }, [entries, filter]);

  const canSubmit = replacement.trim() !== "" && parseTriggers(triggers).length > 0;

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-sm font-medium text-slate-200">Your words</h2>
        <p className="text-xs text-slate-400 mt-1">
          When you say a trigger, Blah³ writes the replacement exactly as you spell it. Replacements
          are also given to the speech model as vocabulary, so it hears them right more often.
        </p>
      </div>

      {fluidAvailable && (
        <button
          onClick={importFluid}
          className="w-full px-3 py-2 text-sm rounded-lg border border-slate-700 bg-slate-800 text-slate-200 hover:border-sky-500 transition-colors"
        >
          Import from FluidVoice
        </button>
      )}

      {/* Add / edit */}
      <form
        className="space-y-2 p-3 rounded-lg bg-slate-800/60 border border-slate-700"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label htmlFor="dict-triggers" className="block text-xs font-medium text-slate-300">
          When I say
        </label>
        <input
          id="dict-triggers"
          value={triggers}
          onChange={(e) => setTriggers(e.target.value)}
          placeholder="mass lab, the mass lab"
          className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-md text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500"
        />
        <label htmlFor="dict-replacement" className="block text-xs font-medium text-slate-300">
          Write
        </label>
        <input
          id="dict-replacement"
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          placeholder="MASS Lab"
          className="w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded-md text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500"
        />
        <p className="text-[11px] text-slate-500">Separate several ways of saying it with commas.</p>
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={!canSubmit}
            className={`flex-1 px-3 py-2 rounded-md text-sm font-medium transition-colors ${
              canSubmit ? "bg-sky-500 hover:bg-sky-600 text-white" : "bg-slate-700 text-slate-400 cursor-not-allowed"
            }`}
          >
            {editingId ? "Save changes" : "Add word"}
          </button>
          {editingId && (
            <button
              type="button"
              onClick={resetForm}
              className="px-3 py-2 rounded-md text-sm text-slate-300 bg-slate-700 hover:bg-slate-600"
            >
              Cancel
            </button>
          )}
        </div>
      </form>

      {notice && (
        <p role="status" className="text-xs text-emerald-400">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="px-3 py-2 text-sm text-red-300 bg-red-500/10 border border-red-500/30 rounded-lg">
          {error}
        </p>
      )}

      {/* List */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-slate-300">
            {entries.length} {entries.length === 1 ? "word" : "words"}
          </span>
          {entries.length > 6 && (
            <input
              aria-label="Search dictionary"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Search"
              className="w-40 px-2 py-1 bg-slate-800 border border-slate-700 rounded-md text-xs text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500"
            />
          )}
        </div>
        {loaded && entries.length === 0 && (
          <p className="text-xs text-slate-500 py-4 text-center">
            No words yet. Add names, slang and terms you use so they come out right.
          </p>
        )}
        <ul className="space-y-2">
          {shown.map((entry) => (
            <li
              key={entry.id}
              className={`p-3 rounded-lg border ${
                editingId === entry.id ? "border-sky-500 bg-sky-500/10" : "border-slate-700 bg-slate-800"
              }`}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="text-sm font-medium text-slate-100 break-words">{entry.replacement}</p>
                  <div className="flex flex-wrap gap-1 mt-1.5">
                    {entry.triggers.map((t) => (
                      <span key={t} className="px-1.5 py-0.5 text-[11px] rounded bg-slate-700 text-slate-300">
                        {t}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="flex gap-1 shrink-0">
                  <button
                    onClick={() => edit(entry)}
                    aria-label={`Edit ${entry.replacement}`}
                    className="px-2 py-1 text-xs rounded text-slate-300 hover:bg-slate-700"
                  >
                    Edit
                  </button>
                  <button
                    onClick={() => remove(entry.id)}
                    aria-label={`Delete ${entry.replacement}`}
                    className="px-2 py-1 text-xs rounded text-red-300 hover:bg-red-500/10"
                  >
                    Delete
                  </button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      </div>

      {/* Try it */}
      <div className="space-y-2">
        <label htmlFor="dict-try" className="block text-xs font-medium text-slate-300">
          Try it
        </label>
        <input
          id="dict-try"
          value={tryText}
          onChange={(e) => setTryText(e.target.value)}
          placeholder="Type a sentence the way you'd say it"
          className="w-full px-3 py-2 bg-slate-800 border border-slate-700 rounded-md text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-sky-500"
        />
        {tryResult && (
          <p data-testid="dict-try-result" className="px-3 py-2 text-sm text-slate-100 bg-slate-900 rounded-md border border-slate-700">
            {tryResult}
          </p>
        )}
      </div>
    </div>
  );
}
