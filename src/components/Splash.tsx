import { useCallback, useEffect, useRef, useState } from "react";

type Phase = "talk" | "merge" | "fade";

const MERGE_AT = 1250; // the three blahs have all landed
const FADE_AT = 1750; // B³ mark has settled
const DONE_AT = 2050; // fade finished

/** No matchMedia (e.g. jsdom) is treated like reduced motion: nothing to animate. */
function shouldSkip(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return true;
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Launch animation: "blah blah blah" pops in, then collapses into the B³ mark.
 * Calls onDone exactly once — when finished, skipped (click / any key), or
 * immediately under prefers-reduced-motion.
 */
export default function Splash({ onDone }: { onDone: () => void }) {
  const [phase, setPhase] = useState<Phase>("talk");
  const [skip] = useState(shouldSkip);
  const doneRef = useRef(false);

  const finish = useCallback(() => {
    if (doneRef.current) return;
    doneRef.current = true;
    onDone();
  }, [onDone]);

  useEffect(() => {
    if (skip) {
      finish();
      return;
    }
    const timers = [
      setTimeout(() => setPhase("merge"), MERGE_AT),
      setTimeout(() => setPhase("fade"), FADE_AT),
      setTimeout(finish, DONE_AT),
    ];
    window.addEventListener("keydown", finish);
    return () => {
      timers.forEach(clearTimeout);
      window.removeEventListener("keydown", finish);
    };
  }, [skip, finish]);

  if (skip) return null;

  return (
    <div
      data-testid="splash"
      data-phase={phase}
      role="presentation"
      onClick={finish}
      className={`splash fixed inset-0 z-50 flex cursor-default select-none items-center justify-center bg-canvas ${
        phase === "fade" ? "splash-fade" : ""
      }`}
    >
      <div className="relative flex h-28 w-80 items-center justify-center">
        <div className={`splash-words ${phase === "talk" ? "" : "splash-words-merge"}`} aria-hidden="true">
          {[0, 1, 2].map((i) => (
            <span key={i} className="splash-word" style={{ animationDelay: `${i * 300}ms` }}>
              blah
            </span>
          ))}
        </div>
        <div className={`splash-mark ${phase === "talk" ? "" : "splash-mark-in"}`} role="img" aria-label="Blah³">
          <span className="text-[56px] font-extrabold leading-none tracking-tight">B</span>
          <sup className="-mt-3 ml-0.5 text-[30px] font-extrabold leading-none">3</sup>
        </div>
      </div>
    </div>
  );
}
