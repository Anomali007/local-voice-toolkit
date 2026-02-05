import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Voice {
  id: string;
  name: string;
  language: string;
  gender: string;
}

export function useTTS() {
  const [isSpeaking, setIsSpeaking] = useState(false);
  const [currentText, setCurrentText] = useState<string | null>(null);
  const [voices, setVoices] = useState<Voice[]>([]);
  const [selectedVoice, setSelectedVoice] = useState("af_heart");
  const [speed, setSpeed] = useState(1.0);
  const [error, setError] = useState<string | null>(null);

  // Listen for hotkey events from the backend
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // TTS started (via hotkey)
      const unlisten1 = await listen<string>("tts-started", (event) => {
        console.log("TTS started via hotkey:", event.payload);
        setIsSpeaking(true);
        setCurrentText(event.payload);
        setError(null);
      });
      unlisteners.push(unlisten1);

      // TTS finished
      const unlisten2 = await listen("tts-finished", () => {
        console.log("TTS finished");
        setIsSpeaking(false);
      });
      unlisteners.push(unlisten2);

      // TTS error
      const unlisten3 = await listen<string>("tts-error", (event) => {
        console.error("TTS error:", event.payload);
        setError(event.payload);
        setIsSpeaking(false);
      });
      unlisteners.push(unlisten3);
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    loadVoices();
    loadSettings();
  }, []);

  const loadVoices = async () => {
    try {
      const result = await invoke<Voice[]>("get_voices");
      setVoices(result);
    } catch (err) {
      console.error("Failed to load voices:", err);
    }
  };

  const loadSettings = async () => {
    try {
      const settings = await invoke<{ tts_voice: string; tts_speed: number }>("get_settings");
      setSelectedVoice(settings.tts_voice);
      setSpeed(settings.tts_speed);
    } catch (err) {
      console.error("Failed to load settings:", err);
    }
  };

  const speak = useCallback(
    async (text: string) => {
      try {
        setError(null);
        setIsSpeaking(true);

        const modelPath = `${getModelsDir()}/tts/kokoro-v1.0.onnx`;

        await invoke("speak_text", {
          text,
          voiceId: selectedVoice,
          speed,
          modelPath,
        });
      } catch (err) {
        setError(String(err));
        console.error("Failed to speak:", err);
      } finally {
        setIsSpeaking(false);
      }
    },
    [selectedVoice, speed]
  );

  const stop = useCallback(async () => {
    try {
      await invoke("stop_speaking");
      setIsSpeaking(false);
    } catch (err) {
      console.error("Failed to stop speaking:", err);
    }
  }, []);

  const updateVoice = useCallback(async (voiceId: string) => {
    setSelectedVoice(voiceId);
    try {
      const settings = await invoke<Record<string, unknown>>("get_settings");
      await invoke("update_settings", {
        settings: { ...settings, tts_voice: voiceId },
      });
    } catch (err) {
      console.error("Failed to save voice setting:", err);
    }
  }, []);

  const updateSpeed = useCallback(async (newSpeed: number) => {
    setSpeed(newSpeed);
    try {
      const settings = await invoke<Record<string, unknown>>("get_settings");
      await invoke("update_settings", {
        settings: { ...settings, tts_speed: newSpeed },
      });
    } catch (err) {
      console.error("Failed to save speed setting:", err);
    }
  }, []);

  return {
    isSpeaking,
    currentText,
    voices,
    selectedVoice,
    speed,
    error,
    speak,
    stop,
    setSelectedVoice: updateVoice,
    setSpeed: updateSpeed,
  };
}

function getModelsDir(): string {
  return "$HOME/Library/Application Support/com.blahcubed.app/models";
}
