import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import ReadingOverlay, { type TtsProgress } from "./ReadingOverlay";

type OverlayState = "recording" | "transcribing" | "result" | "error";
type OverlayMode = "dictation" | "reading";

/** Type/window scale for each size preset; mirrors `size_scale` in overlay.rs. */
const SIZE_SCALE: Record<string, number> = { compact: 0.85, regular: 1, large: 1.2 };
const DEFAULT_LINES = 4;

interface OverlaySettings {
  overlay_size?: string;
  overlay_max_lines?: number;
  tts_speed?: number;
}

interface FrontmostAppInfo {
  name: string;
  bundle_id: string;
}

interface RecordingStartedPayload {
  target_app: FrontmostAppInfo | null;
}

/** Keep the newest words visible: show only the end of a long transcript. */
export function liveTail(text: string, max = 220): string {
  if (text.length <= max) return text;
  const cut = text.slice(text.length - max);
  const space = cut.indexOf(" ");
  return "…" + (space > 0 ? cut.slice(space) : cut);
}

/** Characters of live transcript worth keeping for a given line budget. */
export function tailChars(lines: number): number {
  return lines * 55;
}

export default function DictationOverlay() {
  const [mode, setMode] = useState<OverlayMode>("dictation");
  const [scale, setScale] = useState(1);
  const [lines, setLines] = useState(DEFAULT_LINES);
  const [speed, setSpeed] = useState(1);
  const [progress, setProgress] = useState<TtsProgress | null>(null);
  const [state, setState] = useState<OverlayState>("recording");
  const [targetApp, setTargetApp] = useState<string | null>(null);
  const [result, setResult] = useState<string>("");
  const [error, setError] = useState<string>("");
  const [partialResult, setPartialResult] = useState<string>("");
  // Hold-Space pause (stt-paused / stt-resumed from the backend).
  const [paused, setPaused] = useState(false);
  const [elapsedTime, setElapsedTime] = useState(0);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const glowRef = useRef<HTMLDivElement>(null);
  const levelRef = useRef(0);
  const animationRef = useRef<number | null>(null);
  const levelHistoryRef = useRef<number[]>(new Array(40).fill(0));
  const startTimeRef = useRef<number>(Date.now());
  const elapsedTimeRef = useRef(0);

  // Size and line count come from settings; re-read each time the overlay is
  // about to be used so changes apply without a restart.
  const loadSettings = async () => {
    try {
      const s = await invoke<OverlaySettings>("get_settings");
      if (!s) return;
      setScale(SIZE_SCALE[s.overlay_size ?? "regular"] ?? 1);
      const n = Math.round(s.overlay_max_lines ?? DEFAULT_LINES);
      setLines(Math.min(8, Math.max(2, n)));
      if (typeof s.tts_speed === "number") setSpeed(s.tts_speed);
    } catch (err) {
      console.error("Failed to load overlay settings:", err);
    }
  };

  // Event listeners
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen<boolean>("tts-reading", (event) => {
          if (event.payload) {
            setMode("reading");
            setProgress(null);
            loadSettings();
          } else {
            setMode("dictation");
          }
        })
      );

      unlisteners.push(
        await listen<TtsProgress>("tts-progress", (event) => {
          setProgress(event.payload);
          setSpeed(event.payload.speed);
        })
      );

      unlisteners.push(
        await listen<RecordingStartedPayload>("stt-recording-started", (event) => {
          setMode("dictation");
          loadSettings();
          setState("recording");
          setPaused(false);
          setResult("");
          setError("");
          setPartialResult("");
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
          setPaused(false);
        })
      );

      unlisteners.push(await listen("stt-paused", () => setPaused(true)));
      unlisteners.push(await listen("stt-resumed", () => setPaused(false)));

      unlisteners.push(
        await listen("stt-transcribing", () => {
          setPaused(false);
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
        await listen<string>("stt-partial-result", (event) => {
          setPartialResult(event.payload || "");
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

  // Elapsed time counter during recording (frozen while paused: the clock
  // shows recorded time, so the paused stretch is skipped).
  useEffect(() => {
    if (state !== "recording" || paused) return;

    // Shift the start so the time already shown keeps counting from there.
    startTimeRef.current = Date.now() - elapsedTimeRef.current * 1000;
    const interval = setInterval(() => {
      setElapsedTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [state, paused]);

  // Waveform + glow, driven by real audio levels. Only transforms/opacity are
  // touched per frame, and the loop only runs while recording.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || state !== "recording") {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
      levelRef.current = 0;
      return;
    }

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    const bars = 40;
    const gap = 6;
    const barWidth = (canvas.width - (bars - 1) * gap) / bars;
    const maxHeight = canvas.height;

    const fill = ctx.createLinearGradient(0, 0, canvas.width, 0);
    fill.addColorStop(0, "#22d3ee");
    fill.addColorStop(0.45, "#8b7bff");
    fill.addColorStop(0.75, "#ff4d8d");
    fill.addColorStop(1, "#ff8a3d");

    const animate = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const history = levelHistoryRef.current;

      for (let i = 0; i < bars; i++) {
        // Normalize: typical RMS is 0..0.3, map to 0..1 with clamp
        const normalized = Math.min((history[i] || 0) / 0.15, 1.0);
        const barHeight = Math.max(barWidth, normalized * maxHeight);
        const x = i * (barWidth + gap);
        const y = (canvas.height - barHeight) / 2;

        ctx.globalAlpha = 0.4 + 0.6 * normalized;
        ctx.fillStyle = fill;
        ctx.shadowColor = "rgba(255, 90, 150, 0.65)";
        ctx.shadowBlur = normalized * 14;
        ctx.beginPath();
        ctx.roundRect(x, y, barWidth, barHeight, barWidth / 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
      ctx.shadowBlur = 0;

      // Glow follows the newest few samples, eased so it breathes instead of flickers.
      const recent = history.slice(-3);
      const target = Math.min((recent.reduce((a, b) => a + b, 0) / recent.length) / 0.15, 1);
      levelRef.current += (target - levelRef.current) * 0.25;
      const glow = glowRef.current;
      if (glow) {
        const l = reduceMotion ? 0.4 : levelRef.current;
        glow.style.opacity = String(0.4 + 0.6 * l);
        glow.style.transform = `scale(${1 + 0.07 * l})`;
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

  elapsedTimeRef.current = elapsedTime;

  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  };

  const palette = mode === "reading" ? READING_PALETTE : PALETTES[state];
  const clamp = {
    display: "-webkit-box",
    WebkitBoxOrient: "vertical" as const,
    WebkitLineClamp: lines,
    overflow: "hidden",
  };

  return (
    <div className="flex h-full w-full items-center justify-center p-4" style={{ zoom: scale }}>
      <div className="relative w-full max-w-[368px]">
        {/* Colour bloom behind the card */}
        <div
          ref={glowRef}
          aria-hidden="true"
          className={`absolute -inset-1.5 rounded-[30px] blur-[20px] ${mode === "dictation" && state === "recording" ? "" : "overlay-breathe"}`}
          style={{ backgroundImage: palette, backgroundSize: "300% 100%", opacity: 0.5 }}
        />

        {/* Gradient ring */}
        <div
          className="overlay-ring relative rounded-[23px] p-[1.5px]"
          style={{ backgroundImage: palette, backgroundSize: "300% 100%" }}
        >
          <div
            className="overflow-hidden rounded-[22px] text-white"
            style={{
              background: "rgba(12, 14, 24, 0.84)",
              backdropFilter: "blur(30px) saturate(1.7)",
              WebkitBackdropFilter: "blur(30px) saturate(1.7)",
            }}
          >
            {mode === "reading" ? (
              <ReadingOverlay progress={progress} speed={speed} onSpeedChange={setSpeed} />
            ) : (
              <>
            {/* Status row */}
            <div className="flex items-center justify-between px-4 pb-1 pt-3">
              <div className="flex items-center gap-2">
                {state === "recording" && paused ? (
                  <span
                    className="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide"
                    style={{ background: "rgba(255, 194, 71, 0.16)", color: DOTS.transcribing }}
                  >
                    <span className="flex gap-[2px]" aria-hidden="true">
                      <span className="h-2.5 w-[2.5px] rounded-sm bg-current" />
                      <span className="h-2.5 w-[2.5px] rounded-sm bg-current" />
                    </span>
                    Paused
                  </span>
                ) : state === "recording" ? (
                  <span className="live-chip">
                    <span className="live-dot" />
                    Live
                  </span>
                ) : (
                  <span className="h-2 w-2 rounded-full" style={{ background: DOTS[state] }} />
                )}
                {!(state === "recording" && paused) && (
                  <span className="text-[12px] font-semibold tracking-[-0.005em] text-white/90">{getStateLabel(state)}</span>
                )}
                {targetApp && state === "recording" && (
                  <span className="max-w-[130px] truncate text-[12px] text-white/45">in {targetApp}</span>
                )}
              </div>
              {state === "recording" && (
                <span className="font-mono text-[12px] tabular-nums text-white/60">{formatTime(elapsedTime)}</span>
              )}
            </div>

            {/* Waveform / status glyph */}
            {state === "recording" && (
              <div className="px-4 pt-1.5">
                <canvas
                  ref={canvasRef}
                  width={672}
                  height={80}
                  className="h-10 w-full transition-opacity duration-150"
                  style={{ opacity: paused ? 0.3 : 1 }}
                />
              </div>
            )}
            {state === "transcribing" && (
              <div className="flex items-center gap-1.5 px-4 pt-2.5">
                {[0, 150, 300].map((d) => (
                  <span
                    key={d}
                    className="h-1.5 w-1.5 animate-pulse rounded-full"
                    style={{ animationDelay: `${d}ms`, background: DOTS.transcribing }}
                  />
                ))}
                <span className="ml-1 text-[12px] text-white/60">{partialResult ? "Transcribing..." : "Processing audio..."}</span>
              </div>
            )}
            {state === "result" && (
              <div className="flex items-center gap-1.5 px-4 pt-2.5" style={{ color: DOTS.result }}>
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2.4} aria-hidden="true">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M5 12.5l4.5 4.5L19 7.5" />
                </svg>
                <span className="text-[12px] font-medium">Done</span>
              </div>
            )}
            {state === "error" && (
              <div className="flex items-center gap-1.5 px-4 pt-2.5" style={{ color: DOTS.error }}>
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2} aria-hidden="true">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 4l9 16H3L12 4zM12 10v4.5M12 17.5v.01" />
                </svg>
                <span className="text-[12px] font-medium">Something went wrong</span>
              </div>
            )}

            {/* Transcript: the point of the overlay */}
            <div className="min-h-[56px] px-4 pb-4 pt-2.5">
              {state === "recording" &&
                (partialResult ? (
                  <div style={{ opacity: paused ? 0.55 : 1 }}>
                    <p style={clamp} className="break-words text-[17px] font-semibold leading-snug tracking-[-0.012em] text-white">
                      {liveTail(partialResult, tailChars(lines))}
                    </p>
                    {paused && <p className="mt-1.5 text-[12px] text-white/70">Paused · release Space to continue</p>}
                  </div>
                ) : paused ? (
                  <p className="text-[12px] text-white/55">Paused · release Space to continue</p>
                ) : (
                  <p className="text-[12px] text-white/55">
                    Listening… tap the hotkey again to finish and paste · Esc to cancel
                  </p>
                ))}
              {state === "transcribing" &&
                (partialResult ? (
                  <p style={clamp} className="break-words text-[17px] font-semibold leading-snug tracking-[-0.012em] text-white/80">
                    {partialResult}
                  </p>
                ) : (
                  <p className="text-[12px] text-white/55">Transcribing your speech...</p>
                ))}
              {state === "result" && (
                <p style={clamp} className="break-words text-[17px] font-semibold leading-snug tracking-[-0.012em] text-white">
                  {result || "No speech detected"}
                </p>
              )}
              {state === "error" && (
                <p className="line-clamp-3 break-words text-[13px] leading-snug" style={{ color: DOTS.error }}>
                  {error}
                </p>
              )}
            </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** Each state has its own related-but-distinct colour story. */
const PALETTES: Record<OverlayState, string> = {
  recording: "linear-gradient(90deg, #ff8a3d, #ff3d7f, #8b5cf6, #22b8f7, #ff8a3d)",
  transcribing: "linear-gradient(90deg, #ffd25a, #ff9f1c, #ff6b9a, #ffd25a)",
  result: "linear-gradient(90deg, #34e0a1, #22b8f7, #5a8cf0, #34e0a1)",
  error: "linear-gradient(90deg, #ff5d73, #ff3d3d, #c13a8a, #ff5d73)",
};

/** Reading is the calm sibling of the hot-mic look: cool teal and blue, no warm hues. */
const READING_PALETTE = "linear-gradient(90deg, #5eead4, #38bdf8, #818cf8, #5eead4)";

const DOTS: Record<OverlayState, string> = {
  recording: "#ff4d6d",
  transcribing: "#ffc247",
  result: "#34e0a1",
  error: "#ff6b7d",
};

function getStateLabel(state: OverlayState) {
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
}
