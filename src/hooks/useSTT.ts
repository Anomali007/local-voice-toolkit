import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface TranscriptionResult {
  text: string;
  duration_ms: number;
}

export function useSTT() {
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Listen for hotkey events from the backend
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // STT recording started (via hotkey)
      const unlisten1 = await listen("stt-recording-started", () => {
        console.log("STT recording started via hotkey");
        setIsRecording(true);
        setError(null);
      });
      unlisteners.push(unlisten1);

      // STT recording stopped (via hotkey)
      const unlisten2 = await listen("stt-recording-stopped", () => {
        console.log("STT recording stopped via hotkey");
        setIsRecording(false);
      });
      unlisteners.push(unlisten2);

      // STT transcribing
      const unlisten3 = await listen("stt-transcribing", () => {
        console.log("STT transcribing...");
        setIsTranscribing(true);
      });
      unlisteners.push(unlisten3);

      // STT result
      const unlisten4 = await listen<string>("stt-result", (event) => {
        console.log("STT result:", event.payload);
        setTranscript(event.payload);
        setIsTranscribing(false);
      });
      unlisteners.push(unlisten4);

      // STT error
      const unlisten5 = await listen<string>("stt-error", (event) => {
        console.error("STT error:", event.payload);
        setError(event.payload);
        setIsRecording(false);
        setIsTranscribing(false);
      });
      unlisteners.push(unlisten5);
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  // Manual start recording (from UI button)
  const startRecording = useCallback(async () => {
    try {
      setError(null);
      await invoke("start_recording");
      setIsRecording(true);
    } catch (err) {
      setError(String(err));
      console.error("Failed to start recording:", err);
    }
  }, []);

  // Manual stop recording (from UI button)
  const stopRecording = useCallback(async () => {
    try {
      setIsRecording(false);
      setIsTranscribing(true);
      setError(null);

      const audioData = await invoke<number[]>("stop_recording");

      // Get settings for model path
      const settings = await invoke<{ stt_model: string }>("get_settings");
      const modelPath = `${getModelsDir()}/stt/${settings.stt_model}`;

      const result = await invoke<TranscriptionResult>("transcribe_audio", {
        audioData,
        modelPath,
      });

      setTranscript(result.text);
    } catch (err) {
      setError(String(err));
      console.error("Failed to stop recording or transcribe:", err);
    } finally {
      setIsTranscribing(false);
    }
  }, []);

  const clearTranscript = useCallback(() => {
    setTranscript(null);
    setError(null);
  }, []);

  return {
    isRecording,
    isTranscribing,
    transcript,
    error,
    startRecording,
    stopRecording,
    clearTranscript,
  };
}

function getModelsDir(): string {
  // This should match the Rust backend's model directory
  // On macOS: ~/Library/Application Support/com.blahcubed.app/models
  return "$HOME/Library/Application Support/com.blahcubed.app/models";
}
