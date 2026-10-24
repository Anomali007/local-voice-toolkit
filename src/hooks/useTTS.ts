import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Voice {
  id: string;
  name: string;
  language: string;
  gender: string;
}

export type TtsEngine = "kokoro" | "phonon";

export const PHONON_MODEL_ID = "phonon-31m-en";

/** "Option+S" -> "⌥S", "CommandOrControl+Shift+S" -> "⌘⇧S" */
export function formatHotkey(hotkey: string): string {
  const symbols: Record<string, string> = {
    command: "⌘",
    commandorcontrol: "⌘",
    cmd: "⌘",
    super: "⌘",
    control: "⌃",
    ctrl: "⌃",
    option: "⌥",
    alt: "⌥",
    shift: "⇧",
  };
  return hotkey
    .split("+")
    .map((part) => symbols[part.trim().toLowerCase()] ?? part.trim().toUpperCase())
    .join("");
}

export function useTTS() {
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [currentText, setCurrentText] = useState<string | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [selectedVoice, setSelectedVoice] = useState("af_heart");
  const [engine, setEngineState] = useState<TtsEngine>("kokoro");
  const [phononReady, setPhononReady] = useState(false);
  const [hotkey, setHotkey] = useState("Option+S");
  const [speed, setSpeed] = useState(1.0);
  const [error, setError] = useState<string | null>(null);

  // Listen for hotkey events from the backend
  useEffect(() => {
    const unlisteners: (() => void)[] = [];
    let cancelled = false;

    const setupListeners = async () => {
      const handlers = [
        // TTS started (via hotkey)
        await listen<string>("tts-started", (event) => {
          setIsSpeaking(true);
          setCurrentText(event.payload);
          setError(null);
        }),
        // TTS finished (playback ended or was stopped)
        await listen("tts-finished", () => {
          setIsSpeaking(false);
        }),
        await listen<string>("tts-error", (event) => {
          setError(event.payload);
          setIsSpeaking(false);
        }),
      ];
      if (cancelled) handlers.forEach((unlisten) => unlisten());
      else unlisteners.push(...handlers);
    };

    setupListeners();

    return () => {
      cancelled = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  const loadVoices = useCallback(async (forEngine: TtsEngine) => {
    try {
      const result = await invoke<Voice[]>("get_voices", { engine: forEngine });
      setVoices(result);
      return result;
    } catch (err) {
      console.error("Failed to load voices:", err);
      return [];
    }
  }, []);

  const refreshPhononStatus = useCallback(async () => {
    try {
      const status = await invoke<string>("get_model_status", { modelId: PHONON_MODEL_ID });
      setPhononReady(status === "downloaded");
    } catch {
      setPhononReady(false);
    }
  }, []);

  const [settingsVersion, setSettingsVersion] = useState(0);

  // Engine or speed changed from the menu bar menu: reload.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("settings-changed", () => setSettingsVersion((v) => v + 1)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const load = async () => {
      try {
        const settings = await invoke<{
          tts_voice: string;
          tts_speed: number;
          tts_engine?: string;
          tts_hotkey?: string;
        }>("get_settings");
        const savedEngine: TtsEngine = settings.tts_engine === "phonon" ? "phonon" : "kokoro";
        setEngineState(savedEngine);
        setSelectedVoice(settings.tts_voice);
        setSpeed(settings.tts_speed);
        if (settings.tts_hotkey) setHotkey(settings.tts_hotkey);
        await loadVoices(savedEngine);
      } catch (err) {
        console.error("Failed to load settings:", err);
        await loadVoices("kokoro");
      }
      await refreshPhononStatus();
    };
    load();
  }, [loadVoices, refreshPhononStatus, settingsVersion]);

  const saveSettings = useCallback(async (patch: Record<string, unknown>) => {
    const settings = await invoke<Record<string, unknown>>("get_settings");
    await invoke("update_settings", { settings: { ...settings, ...patch } });
  }, []);

  const speak = useCallback(
    async (text: string) => {
      try {
        setError(null);
        setIsSpeaking(true);
        // Resolves when playback finishes or is stopped.
        await invoke("speak_text", { text, voiceId: selectedVoice, speed, engine });
      } catch (err) {
        setError(String(err));
      } finally {
        setIsSpeaking(false);
      }
    },
    [selectedVoice, speed, engine]
  );

  const stop = useCallback(async () => {
    try {
      await invoke("stop_speaking");
      setIsSpeaking(false);
    } catch (err) {
      console.error("Failed to stop speaking:", err);
    }
  }, []);

  const updateVoice = useCallback(
    async (voiceId: string) => {
      setSelectedVoice(voiceId);
      try {
        await saveSettings({ tts_voice: voiceId });
      } catch (err) {
        console.error("Failed to save voice setting:", err);
      }
    },
    [saveSettings]
  );

  const updateEngine = useCallback(
    async (next: TtsEngine) => {
      setEngineState(next);
      setError(null);
      const nextVoices = await loadVoices(next);
      const patch: Record<string, unknown> = { tts_engine: next };
      // Keep the voice valid for the engine (Phonon has one voice).
      if (nextVoices.length && !nextVoices.some((v) => v.id === selectedVoice)) {
        const voice = next === "kokoro" ? "af_heart" : nextVoices[0].id;
        setSelectedVoice(voice);
        patch.tts_voice = voice;
      }
      try {
        await saveSettings(patch);
      } catch (err) {
        console.error("Failed to save engine setting:", err);
      }
      if (next === "phonon") await refreshPhononStatus();
    },
    [loadVoices, refreshPhononStatus, saveSettings, selectedVoice]
  );

  const updateSpeed = useCallback(
    async (newSpeed: number) => {
      setSpeed(newSpeed);
      try {
        await saveSettings({ tts_speed: newSpeed });
      } catch (err) {
        console.error("Failed to save speed setting:", err);
      }
    },
    [saveSettings]
  );

  return {
    isSpeaking,
    currentText,
    voices,
    selectedVoice,
    engine,
    phononReady,
    hotkey,
    speed,
    error,
    speak,
    stop,
    setSelectedVoice: updateVoice,
    setEngine: updateEngine,
    setSpeed: updateSpeed,
  };
}
