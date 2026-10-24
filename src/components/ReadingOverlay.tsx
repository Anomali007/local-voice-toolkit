import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StopIcon } from "./ui";

export interface TtsProgress {
  elapsed_s: number;
  total_s: number;
  total_estimated: boolean;
  speed: number;
  chunk_index: number;
  chunk_count: number;
}

export const MIN_SPEED = 0.5;
export const MAX_SPEED = 3.0;
export const SPEED_STEP = 0.1;

/** 75 -> "1:15" */
export function formatClock(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  return `${Math.floor(s / 60)}:${(s % 60).toString().padStart(2, "0")}`;
}

/** One step up or down, kept on the 0.1 grid and inside the supported range. */
export function stepSpeed(speed: number, direction: 1 | -1): number {
  const next = Math.round((speed + direction * SPEED_STEP) * 10) / 10;
  return Math.min(MAX_SPEED, Math.max(MIN_SPEED, next));
}

const ACCENT = "#5eead4";

interface Props {
  progress: TtsProgress | null;
  speed: number;
  onSpeedChange: (speed: number) => void;
}

/** "0:42 / 2:10" (total prefixed "~" while it is only an estimate). */
export function formatTimes(progress: TtsProgress | null): string {
  if (!progress || progress.total_s <= 0) return "Preparing…";
  const approx = progress.total_estimated ? "~" : "";
  return `${formatClock(progress.elapsed_s)} / ${approx}${formatClock(progress.total_s)}`;
}

/**
 * The read-aloud pill: stop, speed, a thin progress line and one time label.
 * The window is sized to it, and dragging anywhere that is not a button moves
 * the window (the backend remembers where it was left).
 */
export default function ReadingOverlay({ progress, speed, onSpeedChange }: Props) {
  const known = !!progress && progress.total_s > 0;
  const fraction = known ? Math.min(1, (progress?.elapsed_s ?? 0) / (progress?.total_s ?? 1)) : 0;

  const change = async (direction: 1 | -1) => {
    const target = stepSpeed(speed, direction);
    if (target === speed) return;
    onSpeedChange(target); // optimistic; the backend echoes the stored value
    try {
      const stored = await invoke<number>("set_tts_speed", { speed: target });
      if (typeof stored === "number") onSpeedChange(stored);
    } catch (err) {
      console.error("Failed to change speed:", err);
      onSpeedChange(speed);
    }
  };

  const stop = () => {
    invoke("stop_speaking").catch((err) => console.error("Failed to stop:", err));
  };

  // Native drag: works on the non-activating panel without taking focus from
  // the app being read. Buttons keep their clicks.
  const startDrag = (e: React.MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    getCurrentWindow()
      .startDragging()
      .catch((err) => console.error("Failed to start dragging:", err));
  };

  const round =
    "flex h-7 w-7 items-center justify-center rounded-full bg-white/10 text-[16px] leading-none hover:bg-white/20 disabled:opacity-35";

  return (
    <div
      data-testid="reading-pill"
      onMouseDown={startDrag}
      className="relative flex h-full w-full cursor-grab select-none items-center gap-2.5 overflow-hidden rounded-[18px] px-3 text-white active:cursor-grabbing"
      style={{
        background: "rgba(12, 14, 24, 0.88)",
        boxShadow: `inset 0 0 0 1px ${ACCENT}55`,
        backdropFilter: "blur(30px) saturate(1.7)",
        WebkitBackdropFilter: "blur(30px) saturate(1.7)",
      }}
    >
      <button
        type="button"
        aria-label="Stop"
        title="Stop reading"
        onClick={stop}
        className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full hover:brightness-110"
        style={{ background: ACCENT, color: "#0c0e18" }}
      >
        <StopIcon className="h-3.5 w-3.5" />
      </button>

      <div className="flex shrink-0 items-center gap-1" role="group" aria-label="Reading speed">
        <button type="button" aria-label="Slower" disabled={speed <= MIN_SPEED} onClick={() => change(-1)} className={round}>
          −
        </button>
        <span className="w-10 text-center text-[13px] font-semibold tabular-nums" aria-live="polite">
          {speed.toFixed(1)}×
        </span>
        <button type="button" aria-label="Faster" disabled={speed >= MAX_SPEED} onClick={() => change(1)} className={round}>
          +
        </button>
      </div>

      <span className="ml-auto whitespace-nowrap text-[12px] tabular-nums text-white/75">{formatTimes(progress)}</span>

      <div
        role="progressbar"
        aria-label="Reading progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(fraction * 100)}
        className="absolute inset-x-0 bottom-0 h-[3px] bg-white/10"
      >
        <div
          className="h-full transition-[width] duration-200"
          style={{ width: `${fraction * 100}%`, background: `linear-gradient(90deg, ${ACCENT}, #38bdf8)` }}
        />
      </div>
    </div>
  );
}
