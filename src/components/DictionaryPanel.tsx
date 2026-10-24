import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PageHeader, SearchIcon, Section } from "./ui";

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
      <PageHeader
        title="Your words"
        lede="When you say a trigger, Blah³ writes the replacement exactly as you spell it. Replacements also help the speech model hear them right."
        action={
          fluidAvailable ? (
            <button onClick={importFluid} className="btn-secondary shrink-0">
              Import from FluidVoice
            </button>
          ) : undefined
        }
      />

      {/* Add / edit */}
      <form
        className="card space-y-3 p-4"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label htmlFor="dict-triggers" className="mb-1 block text-xs font-medium text-muted">
              When I say
            </label>
            <input
              id="dict-triggers"
              value={triggers}
              onChange={(e) => setTriggers(e.target.value)}
              placeholder="mass lab, the mass lab"
              className="input"
            />
          </div>
          <div>
            <label htmlFor="dict-replacement" className="mb-1 block text-xs font-medium text-muted">
              Write
            </label>
            <input
              id="dict-replacement"
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              placeholder="MASS Lab"
              className="input"
            />
          </div>
        </div>
        <div className="flex items-center justify-between gap-3">
          <p className="text-xs text-subtle">Separate several ways of saying it with commas.</p>
          <div className="flex shrink-0 gap-2">
            {editingId && (
              <button type="button" onClick={resetForm} className="btn-secondary">
                Cancel
              </button>
            )}
            <button type="submit" disabled={!canSubmit} className="btn-primary">
              {editingId ? "Save changes" : "Add word"}
            </button>
          </div>
        </div>
      </form>

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

      {/* List */}
      <Section>
        <div className="mb-2 flex items-center justify-between px-1">
          <h2 className="text-[13px] font-semibold">
            {entries.length} {entries.length === 1 ? "word" : "words"}
          </h2>
          {entries.length > 6 && (
            <div className="relative">
              <SearchIcon className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-subtle" />
              <input
                aria-label="Search dictionary"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Search"
                className="input w-44 !py-1 !pl-7 !text-xs"
              />
            </div>
          )}
        </div>
        {loaded && entries.length === 0 && (
          <div className="card px-6 py-8 text-center">
            <p className="text-[13px] text-muted">
              No words yet. Add names, slang and terms you use so they come out right.
            </p>
          </div>
        )}
        {shown.length > 0 && (
          <ul className="card card-list overflow-hidden">
            {shown.map((entry) => (
              <li
                key={entry.id}
                className={`flex items-start justify-between gap-3 px-4 py-3 ${
                  editingId === entry.id ? "bg-accent/10" : ""
                }`}
              >
                <div className="min-w-0">
                  <p className="selectable break-words text-[13px] font-medium">{entry.replacement}</p>
                  <div className="mt-1.5 flex flex-wrap gap-1">
                    {entry.triggers.map((t) => (
                      <span key={t} className="badge-neutral">
                        {t}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="flex shrink-0 gap-1">
                  <button onClick={() => edit(entry)} aria-label={`Edit ${entry.replacement}`} className="btn-quiet">
                    Edit
                  </button>
                  <button onClick={() => remove(entry.id)} aria-label={`Delete ${entry.replacement}`} className="btn-danger">
                    Delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Section>

      {/* Try it */}
      <Section>
        <label htmlFor="dict-try" className="section-title block">
          Try it
        </label>
        <input
          id="dict-try"
          value={tryText}
          onChange={(e) => setTryText(e.target.value)}
          placeholder="Type a sentence the way you'd say it"
          className="input"
        />
        {tryResult && (
          <p data-testid="dict-try-result" className="selectable mt-2 rounded-lg bg-accent/10 px-3 py-2 text-[13px]">
            {tryResult}
          </p>
        )}
      </Section>
    </div>
  );
}
