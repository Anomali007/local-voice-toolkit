import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

type Status = "idle" | "recording" | "transcribing" | "speaking" | "error";

export default function StatusIndicator() {
  const [status, setStatus] = useState<Status>("idle");
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // STT events
      const unlisten1 = await listen("stt-recording-started", () => {
        setStatus("recording");
        setMessage("Recording...");
      });
      unlisteners.push(unlisten1);

      const unlisten2 = await listen("stt-recording-stopped", () => {
        setStatus("idle");
        setMessage(null);
      });
      unlisteners.push(unlisten2);

      const unlisten3 = await listen("stt-transcribing", () => {
        setStatus("transcribing");
        setMessage("Transcribing...");
      });
      unlisteners.push(unlisten3);

      const unlisten4 = await listen<string>("stt-result", (event) => {
        setStatus("idle");
        setMessage(`Transcribed: "${event.payload.slice(0, 50)}${event.payload.length > 50 ? "..." : ""}"`);
        // Clear message after 3 seconds
        setTimeout(() => setMessage(null), 3000);
      });
      unlisteners.push(unlisten4);

      const unlisten5 = await listen<string>("stt-error", (event) => {
        setStatus("error");
        setMessage(event.payload);
        setTimeout(() => {
          setStatus("idle");
          setMessage(null);
        }, 5000);
      });
      unlisteners.push(unlisten5);

      // TTS events
      const unlisten6 = await listen<string>("tts-started", () => {
        setStatus("speaking");
        setMessage("Speaking...");
      });
      unlisteners.push(unlisten6);

      const unlisten7 = await listen("tts-finished", () => {
        setStatus("idle");
        setMessage(null);
      });
      unlisteners.push(unlisten7);

      const unlisten8 = await listen<string>("tts-error", (event) => {
        setStatus("error");
        setMessage(event.payload);
        setTimeout(() => {
          setStatus("idle");
          setMessage(null);
        }, 5000);
      });
      unlisteners.push(unlisten8);
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  if (status === "idle" && !message) {
    return null;
  }

  const statusColors = {
    idle: "bg-slate-700",
    recording: "bg-red-500 animate-pulse",
    transcribing: "bg-yellow-500",
    speaking: "bg-sky-500",
    error: "bg-red-600",
  };

  const statusIcons = {
    idle: "✓",
    recording: "🎤",
    transcribing: "⏳",
    speaking: "🔊",
    error: "⚠️",
  };

  return (
    <div className="fixed top-4 right-4 z-50">
      <div
        className={`flex items-center space-x-2 px-4 py-2 rounded-full shadow-lg ${statusColors[status]} text-white text-sm font-medium transition-all`}
      >
        <span>{statusIcons[status]}</span>
        {message && <span className="max-w-xs truncate">{message}</span>}
      </div>
    </div>
  );
}
