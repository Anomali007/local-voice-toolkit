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
  const [elapsedTime, setElapsedTime] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number | null>(null);
  const levelHistoryRef = useRef<number[]>(new Array(40).fill(0));
  const startTimeRef = useRef<number>(Date.now());

  // Event listeners
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen<RecordingStartedPayload>("stt-recording-started", (event) => {
          setState("recording");
          setResult("");
          setError("");
          setElapsedTime(0);
          startTimeRef.current = Date.now();
          levelHistoryRef.current = new Array(40).fill(0);
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

      unlisteners.push(
        await listen<number>("stt-audio-level", (event) => {
          levelHistoryRef.current.push(event.payload);
          if (levelHistoryRef.current.length > 40) {
            levelHistoryRef.current.shift();
          }
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

  // Elapsed time counter during recording
  useEffect(() => {
    if (state !== "recording") return;

    const interval = setInterval(() => {
      setElapsedTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [state]);

  // Canvas waveform animation driven by real audio levels
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

    const bars = 40;
    const gap = 2;
    const barWidth = (canvas.width - (bars - 1) * gap) / bars;
    const maxHeight = canvas.height - 4;

    const animate = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const history = levelHistoryRef.current;

      for (let i = 0; i < bars; i++) {
        // Normalize: typical RMS is 0..0.3, map to 0..1 with clamp
        const rawLevel = history[i] || 0;
        const normalized = Math.min(rawLevel / 0.15, 1.0);
        const barHeight = Math.max(2, normalized * maxHeight);

        const x = i * (barWidth + gap);
        const y = (canvas.height - barHeight) / 2;

        // Color intensity based on level
        const r = Math.floor(200 + normalized * 55);
        ctx.fillStyle = `rgb(${r}, 60, 60)`;
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth, barHeight, 1);
        ctx.fill();
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

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

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

  const getStateLabel = () => {
    switch (state) {
      case "recording":
        return "Recording";
      case "transcribing":
        return "Transcribing";
      case "result":
        return "Transcribed";
      case "error":
        return "Error";
    }
  };

  return (
    <div className="w-full h-full flex items-center justify-center p-3">
      <div
        className={`
          flex flex-col w-full max-w-[380px]
          rounded-2xl border backdrop-blur-xl
          shadow-2xl shadow-black/30
          overflow-hidden
          ${getStatusColor()}
        `}
      >
        {/* Top: Status bar */}
        <div className="flex items-center justify-between px-4 py-2 border-b border-white/5">
          <div className="flex items-center gap-2">
            <div className={`w-2 h-2 rounded-full ${getDotColor()} ${state === "recording" ? "animate-pulse" : ""}`} />
            <span className="text-xs font-medium text-white/90">
              {getStateLabel()}
            </span>
          </div>
          <div className="flex items-center gap-3">
            {targetApp && state === "recording" && (
              <span className="text-[10px] text-white/40 truncate max-w-[120px]">
                {targetApp}
              </span>
            )}
            {state === "recording" && (
              <span className="text-xs text-white/60 font-mono tabular-nums">
                {formatTime(elapsedTime)}
              </span>
            )}
          </div>
        </div>

        {/* Middle: Visualization area */}
        <div className="px-4 py-3 flex items-center justify-center min-h-[64px]">
          {state === "recording" ? (
            <canvas
              ref={canvasRef}
              width={352}
              height={56}
              className="w-full h-14 rounded"
            />
          ) : state === "transcribing" ? (
            <div className="flex items-center gap-2">
              <div className="flex items-center gap-1.5">
                <div className="w-2 h-2 rounded-full bg-amber-500 animate-pulse" />
                <div className="w-2 h-2 rounded-full bg-amber-500 animate-pulse" style={{ animationDelay: "150ms" }} />
                <div className="w-2 h-2 rounded-full bg-amber-500 animate-pulse" style={{ animationDelay: "300ms" }} />
              </div>
              <span className="text-sm text-white/70">Processing audio...</span>
            </div>
          ) : state === "result" ? (
            <div className="flex items-center gap-2">
              <svg className="w-5 h-5 text-green-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
              </svg>
              <span className="text-sm text-green-400 font-medium">Done</span>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <svg className="w-5 h-5 text-red-400 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
              </svg>
              <span className="text-sm text-red-400">Something went wrong</span>
            </div>
          )}
        </div>

        {/* Bottom: Transcript area */}
        <div className="px-4 py-2.5 border-t border-white/5 min-h-[40px] max-h-[72px]">
          {state === "recording" && (
            <p className="text-xs text-white/40 italic">Listening...</p>
          )}
          {state === "transcribing" && (
            <p className="text-xs text-white/40 italic">Transcribing your speech...</p>
          )}
          {state === "result" && (
            <p className="text-sm text-white/90 leading-snug line-clamp-3 break-words">
              {result || "No speech detected"}
            </p>
          )}
          {state === "error" && (
            <p className="text-xs text-red-300 leading-snug line-clamp-3 break-words">
              {error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
