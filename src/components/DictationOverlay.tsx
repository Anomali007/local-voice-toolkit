import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

type OverlayState = "recording" | "transcribing" | "result" | "error";

interface FrontmostAppInfo {
  name: string;
  bundle_id: string;
}

interface RecordingStartedPayload {
  target_app: FrontmostAppInfo | null;
}

export default function DictationOverlay() {
  const [state, setState] = useState<OverlayState>("recording");
  const [targetApp, setTargetApp] = useState<string | null>(null);
  const [result, setResult] = useState<string>("");
  const [error, setError] = useState<string>("");
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number | null>(null);

  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen<RecordingStartedPayload>("stt-recording-started", (event) => {
          setState("recording");
          setResult("");
          setError("");
          if (event.payload?.target_app) {
            setTargetApp(event.payload.target_app.name);
          } else {
            setTargetApp(null);
          }
        })
      );

      unlisteners.push(
        await listen("stt-recording-stopped", () => {
          // Stay in recording state until transcribing starts
        })
      );

      unlisteners.push(
        await listen("stt-transcribing", () => {
          setState("transcribing");
        })
      );

      unlisteners.push(
        await listen<string>("stt-result", (event) => {
          setState("result");
          setResult(event.payload || "");
        })
      );

      unlisteners.push(
        await listen<string>("stt-error", (event) => {
          setState("error");
          setError(event.payload || "Unknown error");
        })
      );
    };

    setup();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, []);

  // Simple waveform animation
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state !== "recording") {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
      return;
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const bars = 12;
    const barWidth = 3;
    const gap = 3;
    const maxHeight = 24;

    const animate = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);

      for (let i = 0; i < bars; i++) {
        // Create a simple animated wave effect
        const time = Date.now() / 150;
        const height = Math.sin(time + i * 0.5) * 0.4 + 0.6;
        const barHeight = height * maxHeight;

        const x = i * (barWidth + gap);
        const y = (canvas.height - barHeight) / 2;

        ctx.fillStyle = "#ef4444"; // red-500
        ctx.fillRect(x, y, barWidth, barHeight);
      }

      animationRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
    };
  }, [state]);

  const getStatusColor = () => {
    switch (state) {
      case "recording":
        return "bg-red-500/20 border-red-500/50";
      case "transcribing":
        return "bg-amber-500/20 border-amber-500/50";
      case "result":
        return "bg-green-500/20 border-green-500/50";
      case "error":
        return "bg-red-500/20 border-red-500/50";
    }
  };

  const getDotColor = () => {
    switch (state) {
      case "recording":
        return "bg-red-500";
      case "transcribing":
        return "bg-amber-500";
      case "result":
        return "bg-green-500";
      case "error":
        return "bg-red-500";
    }
  };

  const getStatusText = () => {
    switch (state) {
      case "recording":
        return "Recording...";
      case "transcribing":
        return "Transcribing...";
      case "result":
        return truncateText(result, 40) || "Done";
      case "error":
        return truncateText(error, 40);
    }
  };

  const truncateText = (text: string, maxLen: number) => {
    if (text.length <= maxLen) return text;
    return text.slice(0, maxLen - 3) + "...";
  };

  return (
    <div className="w-full h-full flex items-center justify-center p-2">
      <div
        className={`
          flex items-center gap-3 px-4 py-3
          rounded-xl border backdrop-blur-xl
          shadow-lg shadow-black/20
          ${getStatusColor()}
        `}
      >
        {/* Status indicator */}
        <div className="flex items-center gap-2">
          {state === "recording" ? (
            <canvas
              ref={canvasRef}
              width={72}
              height={32}
              className="h-8"
            />
          ) : state === "transcribing" ? (
            <div className="flex items-center gap-1">
              <div className={`w-2 h-2 rounded-full ${getDotColor()} animate-pulse`} />
              <div className={`w-2 h-2 rounded-full ${getDotColor()} animate-pulse`} style={{ animationDelay: "150ms" }} />
              <div className={`w-2 h-2 rounded-full ${getDotColor()} animate-pulse`} style={{ animationDelay: "300ms" }} />
            </div>
          ) : (
            <div className={`w-2.5 h-2.5 rounded-full ${getDotColor()}`} />
          )}
        </div>

        {/* Text */}
        <div className="flex flex-col min-w-0">
          {targetApp && state === "recording" && (
            <span className="text-[10px] text-slate-400 truncate">
              Dictating to {targetApp}
            </span>
          )}
          <span className="text-sm text-white font-medium truncate">
            {getStatusText()}
          </span>
        </div>
      </div>
    </div>
  );
}
