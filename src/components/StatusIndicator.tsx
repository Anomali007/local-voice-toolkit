import { useEffect, useState, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import FloatingOverlay from "./FloatingOverlay";

type Status = "idle" | "recording" | "transcribing" | "speaking";

export default function StatusIndicator() {
  const [status, setStatus] = useState<Status>("idle");
  const [resultMessage, setResultMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Stop handler for the overlay
  const handleStop = useCallback(async () => {
    try {
      if (status === "recording") {
        await invoke("stop_recording");
      } else if (status === "speaking") {
        await invoke("stop_speaking");
      }
    } catch (err) {
      console.error("Failed to stop:", err);
    }
  }, [status]);

  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // STT events
      const unlisten1 = await listen("stt-recording-started", () => {
        setStatus("recording");
        setResultMessage(null);
        setErrorMessage(null);
      });
      unlisteners.push(unlisten1);

      const unlisten2 = await listen("stt-recording-stopped", () => {
        // Don't set idle yet - wait for transcription to complete or error
      });
      unlisteners.push(unlisten2);

      const unlisten3 = await listen("stt-transcribing", () => {
        setStatus("transcribing");
      });
      unlisteners.push(unlisten3);

      const unlisten4 = await listen<string>("stt-result", (event) => {
        setStatus("idle");
        const text = event.payload;
        setResultMessage(
          text.length > 60 ? `"${text.slice(0, 60)}..."` : `"${text}"`
        );
        // Clear message after 4 seconds
        setTimeout(() => setResultMessage(null), 4000);
      });
      unlisteners.push(unlisten4);

      const unlisten5 = await listen<string>("stt-error", (event) => {
        setStatus("idle");
        setErrorMessage(event.payload);
        setTimeout(() => setErrorMessage(null), 5000);
      });
      unlisteners.push(unlisten5);

      // TTS events
      const unlisten6 = await listen("tts-started", () => {
        setStatus("speaking");
        setResultMessage(null);
        setErrorMessage(null);
      });
      unlisteners.push(unlisten6);

      const unlisten7 = await listen("tts-finished", () => {
        setStatus("idle");
      });
      unlisteners.push(unlisten7);

      const unlisten8 = await listen<string>("tts-error", (event) => {
        setStatus("idle");
        setErrorMessage(event.payload);
        setTimeout(() => setErrorMessage(null), 5000);
      });
      unlisteners.push(unlisten8);
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  // Map status to overlay mode
  const overlayMode = status === "idle" ? null : status;

  return (
    <>
      {/* Floating overlay for active states */}
      <FloatingOverlay mode={overlayMode} onStop={handleStop} />

      {/* Result toast notification */}
      {resultMessage && status === "idle" && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 animate-in">
          <div className="flex items-center gap-2 px-4 py-3 rounded-full shadow-lg bg-green-500 text-white text-sm font-medium max-w-md">
            <SuccessIcon className="w-5 h-5 flex-shrink-0" />
            <span className="truncate">{resultMessage}</span>
          </div>
        </div>
      )}

      {/* Error toast notification */}
      {errorMessage && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 animate-in">
          <div className="flex items-center gap-2 px-4 py-3 rounded-xl shadow-lg bg-red-500 text-white text-sm font-medium max-w-md">
            <ErrorIcon className="w-5 h-5 flex-shrink-0" />
            <span>{errorMessage}</span>
          </div>
        </div>
      )}
    </>
  );
}

function SuccessIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
    </svg>
  );
}

function ErrorIcon({ className }: { className?: string }) {
  return (
    <svg className={className} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
      />
    </svg>
  );
}
