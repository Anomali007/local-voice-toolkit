import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appDataDir, join } from "@tauri-apps/api/path";

interface TranscriptionResult {
  text: string;
  duration_ms: number;
}

interface StopRecordingResult {
  audio_data: number[];
  silence_triggered: boolean;
}

export function useSTT() {
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [transcript, setTranscript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [silenceTriggered, setSilenceTriggered] = useState(false);
  const silencePollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

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

  // Poll for silence detection auto-stop
  const startSilencePolling = useCallback(() => {
    if (silencePollingRef.current) return;

    silencePollingRef.current = setInterval(async () => {
      try {
        const triggered = await invoke<boolean>("is_silence_triggered");
        if (triggered) {
          console.log("Silence detected - auto-stopping recording");
          setSilenceTriggered(true);
          stopSilencePolling();
          // The actual stop will be handled by the recording check
        }
      } catch {
        // Ignore polling errors
      }
    }, 100); // Poll every 100ms
  }, []);

  const stopSilencePolling = useCallback(() => {
    if (silencePollingRef.current) {
      clearInterval(silencePollingRef.current);
      silencePollingRef.current = null;
    }
  }, []);

  // Auto-stop when silence is detected
  useEffect(() => {
    if (silenceTriggered && isRecording) {
      stopRecording();
    }
  }, [silenceTriggered, isRecording]);

  // Clean up polling on unmount
  useEffect(() => {
    return () => stopSilencePolling();
  }, [stopSilencePolling]);

  // Manual start recording (from UI button)
  const startRecording = useCallback(async () => {
    try {
      setError(null);
      setSilenceTriggered(false);
      await invoke("start_recording");
      setIsRecording(true);
      startSilencePolling();
    } catch (err) {
      setError(String(err));
      console.error("Failed to start recording:", err);
    }
  }, [startSilencePolling]);

  // Manual stop recording (from UI button)
  const stopRecording = useCallback(async () => {
    try {
      stopSilencePolling();
      setIsRecording(false);
      setIsTranscribing(true);
      setError(null);

      const result = await invoke<StopRecordingResult>("stop_recording");

      if (result.silence_triggered) {
        console.log("Recording was auto-stopped by silence detection");
      }

      // Get settings for model path
      const settings = await invoke<{ stt_model: string }>("get_settings");
      const modelsDir = await getModelsDir();
      const modelPath = await join(modelsDir, "stt", settings.stt_model);

      const transcription = await invoke<TranscriptionResult>("transcribe_audio", {
        audioData: result.audio_data,
        modelPath,
      });

      setTranscript(transcription.text);
      setSilenceTriggered(false);
    } catch (err) {
      setError(String(err));
      console.error("Failed to stop recording or transcribe:", err);
    } finally {
      setIsTranscribing(false);
    }
  }, [stopSilencePolling]);

  const clearTranscript = useCallback(() => {
    setTranscript(null);
    setError(null);
  }, []);

  return {
    isRecording,
    isTranscribing,
    transcript,
    error,
    silenceTriggered,
    startRecording,
    stopRecording,
    clearTranscript,
  };
}

async function getModelsDir(): Promise<string> {
  // Use Tauri's path API to get the correct app data directory
  // On macOS: ~/Library/Application Support/com.blahcubed.app/models
  const dataDir = await appDataDir();
  return await join(dataDir, "models");
}
