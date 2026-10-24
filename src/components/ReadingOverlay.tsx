import { invoke } from "@tauri-apps/api/core";
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

/** Inside the overlay card while Blah³ reads aloud: progress, speed, stop. */
export default function ReadingOverlay({ progress, speed, onSpeedChange }: Props) {
  const known = !!progress && progress.total_s > 0;
  const elapsed = progress?.elapsed_s ?? 0;
  const total = progress?.total_s ?? 0;
  const remaining = Math.max(0, total - elapsed);
  const fraction = known ? Math.min(1, elapsed / total) : 0;
  const approx = progress?.total_estimated ? "~" : "";

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

  return (
    <div className="px-4 pb-4 pt-3 text-white">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-2 text-[12px] font-semibold uppercase tracking-[0.08em]" style={{ color: ACCENT }}>
          <span className="h-2 w-2 rounded-full" style={{ background: ACCENT }} />
          Reading aloud
        </span>
        {progress && progress.chunk_count > 1 && (
          <span className="text-[11px] tabular-nums text-white/50">
            {progress.chunk_index} / {progress.chunk_count}
          </span>
        )}
      </div>

      <div className="mt-4">
        <div
          role="progressbar"
          aria-label="Reading progress"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(fraction * 100)}
          className="h-1.5 w-full overflow-hidden rounded-full bg-white/15"
        >
          <div
            className="h-full rounded-full transition-[width] duration-200"
            style={{ width: `${fraction * 100}%`, background: `linear-gradient(90deg, ${ACCENT}, #38bdf8)` }}
          />
        </div>
        <div className="mt-1.5 flex justify-between text-[12px] tabular-nums text-white/70">
          <span>{formatClock(elapsed)}</span>
          <span>{known ? `${approx}-${formatClock(remaining)}` : "Preparing…"}</span>
        </div>
      </div>

      <div className="mt-3 flex items-center justify-between">
        <div className="flex items-center gap-1.5" role="group" aria-label="Reading speed">
          <button
            type="button"
            aria-label="Slower"
            disabled={speed <= MIN_SPEED}
            onClick={() => change(-1)}
            className="flex h-8 w-8 items-center justify-center rounded-full bg-white/10 text-[18px] leading-none hover:bg-white/20 disabled:opacity-35"
          >
            −
          </button>
          <span className="w-12 text-center text-[15px] font-semibold tabular-nums" aria-live="polite">
            {speed.toFixed(1)}×
          </span>
          <button
            type="button"
            aria-label="Faster"
            disabled={speed >= MAX_SPEED}
            onClick={() => change(1)}
            className="flex h-8 w-8 items-center justify-center rounded-full bg-white/10 text-[18px] leading-none hover:bg-white/20 disabled:opacity-35"
          >
            +
          </button>
        </div>
        <button
          type="button"
          onClick={stop}
          className="flex h-8 items-center gap-1.5 rounded-full bg-white/12 px-3.5 text-[13px] font-medium hover:bg-white/22"
        >
          <StopIcon className="h-3.5 w-3.5" />
          Stop
        </button>
      </div>
    </div>
  );
}
