import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { CheckIcon, FlameIcon, PageHeader, Spinner } from "./ui";
import {
  DAILY_GOAL,
  DICTATION_STEPS,
  RANGES,
  STREAK_STEPS,
  TYPING_WPM,
  WORD_STEPS,
  compact,
  cumulative,
  formatMinutes,
  heatmap,
  hourLabel,
  longDate,
  milestone,
  parseDay,
  summarizeRange,
  thisWeek,
  weekdayAverages,
  type Range,
  type Stats,
} from "../lib/stats";

const n = (v: number) => v.toLocaleString();

/* ---------- Tooltip: one per chart, positioned inside the chart's box ---------- */

interface TipState {
  x: number;
  y: number;
  value: string;
  label: string;
}

function useTip() {
  const box = useRef<HTMLDivElement>(null);
  const [tip, setTip] = useState<TipState | null>(null);
  const show = useCallback((target: Element, value: string, label: string, clientX?: number) => {
    const outer = box.current?.getBoundingClientRect();
    if (!outer) return;
    const r = target.getBoundingClientRect();
    const x = (clientX ?? r.left + r.width / 2) - outer.left;
    setTip({ x: Math.min(Math.max(x, 70), outer.width - 70), y: r.top - outer.top, value, label });
  }, []);
  return { box, tip, show, hide: useCallback(() => setTip(null), []) };
}

function Tip({ tip }: { tip: TipState | null }) {
  if (!tip) return null;
  return (
    <div
      role="tooltip"
      className="pointer-events-none absolute z-10 -translate-x-1/2 whitespace-nowrap rounded-lg bg-fg px-2.5 py-1.5 text-canvas shadow-pop"
      style={{ left: tip.x, top: tip.y, transform: "translate(-50%, calc(-100% - 8px))" }}
    >
      <div className="text-[13px] font-semibold">{tip.value}</div>
      <div className="text-[11px] opacity-70">{tip.label}</div>
    </div>
  );
}

/** The element's width in CSS pixels, so charts draw text at true size. */
function useWidth(ref: React.RefObject<HTMLElement>, fallback = 520): number {
  const [width, setWidth] = useState(fallback);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(([e]) => {
      const w = Math.round(e.contentRect.width);
      if (w > 0) setWidth(w);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref, fallback]);
  return width;
}

/* ---------- Building blocks ---------- */

function Card({
  title,
  sub,
  action,
  children,
  className = "",
}: {
  title?: string;
  sub?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`card p-5 ${className}`}>
      {(title || action) && (
        <div className="mb-4 flex items-start justify-between gap-3">
          <div>
            {title && <h2 className="text-[15px] font-semibold tracking-[-0.01em]">{title}</h2>}
            {sub && <p className="mt-0.5 text-xs text-muted">{sub}</p>}
          </div>
          {action}
        </div>
      )}
      {children}
    </section>
  );
}

function Tile({ label, value, note, accent = false }: { label: string; value: string; note: string; accent?: boolean }) {
  return (
    <div className="card p-4">
      <p className="text-[11px] font-semibold uppercase tracking-[0.06em] text-muted">{label}</p>
      <p className={`mt-1.5 text-[26px] font-semibold tracking-[-0.02em] ${accent ? "text-warm" : ""}`}>{value}</p>
      <p className="mt-0.5 text-xs text-subtle">{note}</p>
    </div>
  );
}

/** A bar with a 4px rounded top, square at the baseline. */
function barPath(x: number, y: number, w: number, h: number): string {
  const r = Math.min(4, w / 2, h);
  return `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`;
}

function niceTicks(max: number, count = 3): number[] {
  if (max <= 0) return [0];
  const raw = max / count;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const m = raw / pow;
  const step = (m <= 1 ? 1 : m <= 2 ? 2 : m <= 5 ? 5 : 10) * pow;
  const top = Math.ceil(max / step) * step;
  return Array.from({ length: Math.round(top / step) + 1 }, (_, i) => i * step);
}

/* ---------- Sections ---------- */

function Hero({ stats }: { stats: Stats }) {
  const week = thisWeek(stats);
  const left = Math.max(0, DAILY_GOAL - stats.today_words);
  const pct = Math.min(1, stats.today_words / DAILY_GOAL);
  return (
    <section
      className="card relative overflow-hidden p-6"
      style={{
        backgroundImage:
          "radial-gradient(120% 140% at 100% 0%, rgb(var(--c-warm) / 0.14), transparent 55%), radial-gradient(90% 120% at 0% 100%, rgb(var(--c-accent) / 0.12), transparent 60%)",
      }}
    >
      <div className="flex flex-wrap items-start justify-between gap-6">
        <div className="min-w-[200px] flex-1">
          <h2 className="text-[28px] font-semibold leading-[1.1] tracking-[-0.025em]">
            Every word
            <br />
            counts.
          </h2>
          <p className="mt-2 text-[13px] text-muted">Your dictations, measured on this Mac.</p>
        </div>
        <div>
          <div className="flex items-center gap-2 text-warm">
            <FlameIcon className="h-8 w-8" />
            <span className="text-[44px] font-semibold leading-none tracking-[-0.03em]">{n(stats.current_streak)}</span>
            <span className="self-end pb-1 text-[13px] text-muted">day streak</span>
          </div>
          <p className="mt-1 text-xs text-subtle">Best: {n(stats.best_streak)} days</p>
          <ol className="mt-3 flex gap-2" aria-label="This week">
            {week.map((d) => (
              <li key={d.date} className="flex flex-col items-center gap-1">
                <span
                  aria-label={`${longDate(d.date)}: ${d.active ? "dictated" : d.future ? "still to come" : "no dictation"}`}
                  className={`flex h-6 w-6 items-center justify-center rounded-full ${
                    d.active
                      ? "bg-warm text-canvas"
                      : d.isToday
                        ? "border-[1.5px] border-dashed border-warm/70"
                        : d.future
                          ? "bg-fg/[0.05]"
                          : "border-[1.5px] border-fg/15"
                  }`}
                >
                  {d.active && <CheckIcon className="h-3.5 w-3.5" strokeWidth={2.4} />}
                </span>
                <span className={`text-[10px] ${d.isToday ? "font-semibold text-fg" : "text-subtle"}`}>{d.letter}</span>
              </li>
            ))}
          </ol>
        </div>
      </div>

      <div className="mt-6 border-t border-line/70 pt-5">
        <div className="flex items-baseline justify-between gap-3">
          <p className="text-[17px] font-semibold tracking-[-0.01em]">
            {left > 0 ? `${n(left)} words to today's goal` : `Goal reached — ${n(stats.today_words)} words today`}
          </p>
          <p className="text-[17px] font-semibold text-accent">{Math.round(pct * 100)}%</p>
        </div>
        <div
          className="mt-3 h-2 overflow-hidden rounded-full bg-accent/15"
          role="progressbar"
          aria-label="Today's goal"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(pct * 100)}
        >
          <div className="h-full rounded-full bg-accent transition-[width] duration-500" style={{ width: `${pct * 100}%` }} />
        </div>
        <p className="mt-2 text-xs text-subtle">
          {n(stats.today_dictations)} {stats.today_dictations === 1 ? "dictation" : "dictations"} today · goal {DAILY_GOAL} words
        </p>
      </div>
    </section>
  );
}

const HEAT_FILL = ["fill-fg/[0.07]", "fill-accent/25", "fill-accent/45", "fill-accent/70", "fill-accent"];

function Heatmap({ stats, range }: { stats: Stats; range: Range }) {
  const { weeks, months } = useMemo(() => heatmap(stats, range), [stats, range]);
  const { box, tip, show, hide } = useTip();
  const STEP = 15;
  const CELL = 12;
  const LEFT = 28;
  const TOP = 16;
  const w = LEFT + weeks.length * STEP;
  const h = TOP + 7 * STEP;
  return (
    <div ref={box} className="relative">
      <svg
        viewBox={`0 0 ${w} ${h}`}
        width="100%"
        style={{ maxWidth: w * 1.45 }}
        role="group"
        aria-label="Words per day"
        onPointerLeave={hide}
      >
        {months.map((m) => (
          <text key={`${m.col}-${m.label}`} x={LEFT + m.col * STEP} y={10} className="fill-subtle text-[9px]">
            {m.label}
          </text>
        ))}
        {[
          [0, "Sun"],
          [2, "Tue"],
          [4, "Thu"],
          [6, "Sat"],
        ].map(([row, label]) => (
          <text key={label} x={0} y={TOP + (row as number) * STEP + 9.5} className="fill-subtle text-[9px]">
            {label}
          </text>
        ))}
        {weeks.map((week, col) =>
          week.map((cell, row) =>
            cell.future ? null : (
              <rect
                key={cell.date}
                x={LEFT + col * STEP}
                y={TOP + row * STEP}
                width={CELL}
                height={CELL}
                rx={3}
                className={`${HEAT_FILL[cell.level]} stroke-fg/60 [stroke-width:0] hover:[stroke-width:1.5]`}
                aria-label={`${longDate(cell.date)}: ${n(cell.words)} words`}
                onPointerEnter={(e) =>
                  show(
                    e.currentTarget,
                    `${n(cell.words)} words`,
                    `${longDate(cell.date)}${cell.dictations ? ` · ${n(cell.dictations)} dictations` : ""}`,
                  )
                }
              />
            ),
          ),
        )}
      </svg>
      <div className="mt-2 flex items-center justify-end gap-1.5 text-[11px] text-subtle" aria-hidden="true">
        Less
        {HEAT_FILL.map((f) => (
          <svg key={f} width="10" height="10">
            <rect width="10" height="10" rx="2.5" className={f} />
          </svg>
        ))}
        More
      </div>
      <Tip tip={tip} />
    </div>
  );
}

function Growth({ stats, range }: { stats: Stats; range: Range }) {
  const points = useMemo(() => cumulative(stats, range), [stats, range]);
  const { box, tip, show, hide } = useTip();
  const [hover, setHover] = useState<number | null>(null);
  const W = useWidth(box);
  const H = 180;
  const L = 46;
  const R = 10;
  const T = 10;
  const B = 24;
  const max = points[points.length - 1]?.total ?? 0;
  const ticks = niceTicks(max);
  const top = ticks[ticks.length - 1] || 1;
  const x = (i: number) => L + (points.length > 1 ? (i / (points.length - 1)) * (W - L - R) : 0);
  const y = (v: number) => T + (1 - v / top) * (H - T - B);
  const line = points.map((p, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(p.total).toFixed(1)}`).join("");
  const area = `${line}L${x(points.length - 1)},${y(0)}L${x(0)},${y(0)}Z`;
  const firsts = points
    .map((p, i) => ({ i, d: parseDay(p.date) }))
    .filter(({ d, i }) => d.getDate() === 1 && i > 3 && i < points.length - 4);
  const maxLabels = Math.max(2, Math.floor(W / 90));
  const labels = firsts.filter((_, k) => k % Math.ceil(firsts.length / maxLabels || 1) === 0);

  const onMove = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const sx = ((e.clientX - r.left) / r.width) * W;
    const i = Math.round(((sx - L) / (W - L - R)) * (points.length - 1));
    const idx = Math.min(points.length - 1, Math.max(0, i));
    setHover(idx);
    const p = points[idx];
    show(e.currentTarget, `${n(p.total)} words`, `${longDate(p.date)} · +${n(p.words)} that day`, r.left + (x(idx) / W) * r.width);
  };

  return (
    <div ref={box} className="relative">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        width="100%"
        role="img"
        aria-label={`Cumulative words: ${n(max)} in this period`}
        onPointerMove={onMove}
        onPointerLeave={() => {
          setHover(null);
          hide();
        }}
        className="touch-none"
      >
        {ticks.map((t) => (
          <g key={t}>
            <line x1={L} x2={W - R} y1={y(t)} y2={y(t)} className="stroke-line" strokeWidth={1} />
            <text x={L - 8} y={y(t) + 3.5} textAnchor="end" className="fill-subtle text-[10px] tabular-nums">
              {compact(t)}
            </text>
          </g>
        ))}
        {labels.map(({ i, d }) => (
          <text key={i} x={x(i)} y={H - 6} textAnchor="middle" className="fill-subtle text-[10px]">
            {d.toLocaleDateString(undefined, { month: "short", day: "numeric" })}
          </text>
        ))}
        <path d={area} className="fill-accent/10" />
        <path d={line} fill="none" className="stroke-accent" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {hover !== null && (
          <>
            <line x1={x(hover)} x2={x(hover)} y1={T} y2={y(0)} className="stroke-fg/30" strokeWidth={1} />
            <circle cx={x(hover)} cy={y(points[hover].total)} r={4.5} className="fill-accent stroke-surface" strokeWidth={2} />
          </>
        )}
      </svg>
      <Tip tip={tip} />
    </div>
  );
}

function Bars({
  values,
  labels,
  tipLabel,
  tipValue,
  ariaLabel,
  height = 150,
  tickEvery = 1,
}: {
  values: number[];
  labels: string[];
  tipLabel: (i: number) => string;
  tipValue: (v: number) => string;
  ariaLabel: string;
  height?: number;
  tickEvery?: number;
}) {
  const { box, tip, show, hide } = useTip();
  const W = useWidth(box);
  const H = height;
  const B = 20;
  const T = 18;
  const max = Math.max(1, ...values);
  const peak = values.indexOf(Math.max(...values));
  const band = W / values.length;
  const bw = Math.min(24, band * 0.56);
  return (
    <div ref={box} className="relative">
      <svg viewBox={`0 0 ${W} ${H}`} width="100%" role="group" aria-label={ariaLabel} onPointerLeave={hide}>
        <line x1={0} x2={W} y1={H - B} y2={H - B} className="stroke-line" strokeWidth={1} />
        {values.map((v, i) => {
          const bh = Math.max(v > 0 ? 2 : 0, (v / max) * (H - B - T));
          const bx = i * band + (band - bw) / 2;
          return (
            <g key={i}>
              {/* Hit target: the whole band, not just the painted bar. */}
              <rect
                x={i * band}
                y={0}
                width={band}
                height={H - B}
                fill="transparent"
                aria-label={`${tipLabel(i)}: ${tipValue(v)}`}
                onPointerEnter={(e) => show(e.currentTarget.nextElementSibling ?? e.currentTarget, tipValue(v), tipLabel(i))}
              />
              <path
                d={barPath(bx, H - B - bh, bw, bh)}
                className={`pointer-events-none ${i === peak ? "fill-accent" : "fill-accent/35"}`}
              />
              {i === peak && v > 0 && (
                <text x={bx + bw / 2} y={H - B - bh - 5} textAnchor="middle" className="fill-fg text-[10px] font-semibold">
                  {tipValue(v).split(" ")[0]}
                </text>
              )}
              {i % tickEvery === 0 && (
                <text x={i * band + band / 2} y={H - 5} textAnchor="middle" className="fill-subtle text-[10px]">
                  {labels[i]}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      <Tip tip={tip} />
    </div>
  );
}

function Ring({ progress, done }: { progress: number; done: boolean }) {
  const r = 24;
  const c = 2 * Math.PI * r;
  return (
    <svg width="60" height="60" viewBox="0 0 60 60" aria-hidden="true" className="-rotate-90 shrink-0">
      <circle cx="30" cy="30" r={r} fill="none" strokeWidth="5" className="stroke-fg/10" />
      <circle
        cx="30"
        cy="30"
        r={r}
        fill="none"
        strokeWidth="5"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - progress)}
        className={done ? "stroke-ok" : "stroke-accent"}
      />
    </svg>
  );
}

function MilestoneCard({ title, value, unit, steps }: { title: string; value: number; unit: string; steps: number[] }) {
  const m = milestone(value, steps);
  const done = m.next === null;
  return (
    <div className="card flex flex-col p-4">
      <div className="flex items-center justify-between text-[13px]">
        <span className="font-semibold">{title}</span>
        <span className="text-xs text-subtle tabular-nums">
          {m.reached}/{m.steps}
        </span>
      </div>
      <div className="mt-3 flex items-center gap-3">
        <Ring progress={m.progress} done={done} />
        <div>
          <p className="text-[22px] font-semibold leading-tight tracking-[-0.02em]">{n(value)}</p>
          <p className="text-xs text-muted">{unit}</p>
        </div>
      </div>
      <p className="mt-3 text-xs text-muted">
        {done
          ? "Every milestone reached."
          : `Next: ${compact(m.next!)} · ${n(m.next! - value)} to go · ${Math.floor(m.progress * 100)}%`}
      </p>
      <div className="mt-3 flex gap-1" aria-hidden="true">
        {steps.map((s) => (
          <span key={s} className={`h-1 flex-1 rounded-full ${value >= s ? (done ? "bg-ok" : "bg-accent") : "bg-fg/10"}`} />
        ))}
      </div>
    </div>
  );
}

function Best({ value, title, note }: { value: string; title: string; note: string }) {
  return (
    <div className="card p-4">
      <p className="text-[11px] font-semibold uppercase tracking-[0.06em] text-warm">Personal best</p>
      <p className="mt-2 text-[30px] font-semibold leading-none tracking-[-0.03em]">{value}</p>
      <p className="mt-2 text-[13px] font-medium">{title}</p>
      <p className="text-xs text-subtle">{note}</p>
    </div>
  );
}

/* ---------- Page ---------- */

export default function StatsPanel() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [range, setRange] = useState<Range>(6);
  const [fluidVoice, setFluidVoice] = useState(false);
  const [importing, setImporting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setStats(await invoke<Stats>("get_history_stats"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    load();
    invoke<boolean>("fluidvoice_history_available")
      .then((v) => setFluidVoice(!!v))
      .catch(() => {});
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("history-updated", () => load()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  const importFluidVoice = async () => {
    setImporting(true);
    setNotice(null);
    try {
      const r = await invoke<{ imported: number; already_here: number }>("import_fluidvoice_history");
      setNotice(r.imported ? `Brought over ${n(r.imported)} dictations from FluidVoice.` : "Already up to date with FluidVoice.");
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  };

  const summary = useMemo(() => (stats ? summarizeRange(stats, range) : null), [stats, range]);
  const weekdays = useMemo(() => (stats ? weekdayAverages(stats, range) : []), [stats, range]);

  if (error && !stats) {
    return (
      <p role="alert" className="alert-danger">
        {error}
      </p>
    );
  }
  if (!stats || !summary) {
    return (
      <div className="flex justify-center py-16">
        <Spinner />
      </div>
    );
  }

  const offerImport = fluidVoice && stats.imported_from_fluidvoice === 0;
  const importCard = offerImport && (
    <section className="card flex flex-wrap items-center justify-between gap-3 p-4">
      <div>
        <p className="text-[13px] font-semibold">Bring your FluidVoice history</p>
        <p className="text-xs text-muted">Every past dictation comes over, so your totals and streak carry on. Window titles are left behind.</p>
      </div>
      <button onClick={importFluidVoice} disabled={importing} className="btn-primary shrink-0">
        {importing ? "Importing…" : "Import"}
      </button>
    </section>
  );

  if (stats.total_dictations === 0) {
    return (
      <div className="space-y-6">
        <PageHeader title="Stats" lede="How much you say, and when. Counted only on this Mac." />
        {importCard}
        <div className="card px-6 py-10 text-center">
          <p className="text-[13px] text-muted">No dictations yet. Hold Right ⌥ and talk — your numbers start here.</p>
        </div>
      </div>
    );
  }

  const minutesSaved = stats.total_words / TYPING_WPM;
  const perDictation = Math.round(stats.total_words / Math.max(1, stats.total_dictations));
  const peakHour = stats.hours.indexOf(Math.max(...stats.hours));
  const total = Math.max(1, stats.total_dictations);

  return (
    <div className="space-y-5">
      <PageHeader title="Stats" lede="How much you say, and when. Counted only on this Mac." />
      {importCard}
      {notice && (
        <p role="status" className="px-1 text-xs text-ok">
          {notice}
        </p>
      )}

      <Hero stats={stats} />

      <Card
        title="A little every day"
        sub="Every square is a day. Hover one for details."
        action={
          <select aria-label="Period" value={range} onChange={(e) => setRange(Number(e.target.value) as Range)}>
            {RANGES.map((r) => (
              <option key={r} value={r}>
                {r === 12 ? "12 months" : r === 1 ? "1 month" : `${r} months`}
              </option>
            ))}
          </select>
        }
      >
        <p className="mb-4">
          <span className="text-[40px] font-semibold leading-none tracking-[-0.03em]">{n(summary.words)}</span>
          <span className="ml-2 text-[13px] text-muted">words in {n(summary.days)} days</span>
        </p>
        <Heatmap stats={stats} range={range} />
        <dl className="mt-4 grid grid-cols-3 gap-3 border-t border-line/70 pt-4">
          <div>
            <dt className="text-xs text-muted">Best day</dt>
            <dd className="text-[17px] font-semibold">{n(summary.best?.words ?? 0)}</dd>
            <dd className="text-[11px] text-subtle">{summary.best ? longDate(summary.best.date) : "—"}</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">Per active day</dt>
            <dd className="text-[17px] font-semibold">{n(summary.perActiveDay)}</dd>
            <dd className="text-[11px] text-subtle">words</dd>
          </div>
          <div>
            <dt className="text-xs text-muted">Active days</dt>
            <dd className="text-[17px] font-semibold">{n(summary.activeDays)}</dd>
            <dd className="text-[11px] text-subtle">of {n(summary.days)}</dd>
          </div>
        </dl>
      </Card>

      <div className="grid gap-5 [grid-template-columns:repeat(auto-fit,minmax(300px,1fr))]">
        <Card title="Words add up" sub="Running total for the period. Hover along the line.">
          <Growth stats={stats} range={range} />
        </Card>
        <Card title="Your weekly rhythm" sub="Average words per weekday, quiet days included.">
          <Bars
            values={weekdays}
            labels={["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]}
            tipLabel={(i) => `Average ${["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][i]}`}
            tipValue={(v) => `${n(v)} words`}
            ariaLabel="Average words per weekday"
            height={180}
          />
        </Card>
      </div>

      <div>
        <h2 className="section-title">All-time</h2>
        <div className="grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]">
          <Tile label="Time saved" value={formatMinutes(minutesSaved)} note={`vs typing at ${TYPING_WPM} words a minute`} />
          <Tile label="Words" value={n(stats.total_words)} note={`${n(Math.round(stats.total_words / Math.max(1, stats.active_days)))} per active day`} />
          <Tile
            label="Active days"
            value={n(stats.active_days)}
            note={stats.days.length ? `since ${longDate(stats.days[0].date)}` : "days you dictated"}
            accent
          />
          <Tile label="Dictations" value={n(stats.total_dictations)} note={`${n(perDictation)} words each on average`} />
          <Tile label="Keystrokes saved" value={n(stats.total_chars)} note="keys you never pressed" />
        </div>
      </div>

      <div>
        <h2 className="section-title">Milestones</h2>
        <div className="grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]">
          <MilestoneCard title="Words spoken" value={stats.total_words} unit="words" steps={WORD_STEPS} />
          <MilestoneCard title="Ideas captured" value={stats.total_dictations} unit="dictations" steps={DICTATION_STEPS} />
          <MilestoneCard title="Showing up" value={stats.best_streak} unit="days in your best streak" steps={STREAK_STEPS} />
        </div>
      </div>

      <div className="grid gap-5 [grid-template-columns:repeat(auto-fit,minmax(300px,1fr))]">
        <Card title="Where your words go" sub="Dictations per app.">
          <ul className="space-y-3">
            {stats.apps.slice(0, 6).map((a) => {
              const share = a.dictations / total;
              return (
                <li key={a.name}>
                  <div className="flex justify-between gap-3 text-[13px]">
                    <span className="truncate font-medium">{a.name}</span>
                    <span className="shrink-0 text-xs text-muted tabular-nums">
                      {n(a.dictations)} · {Math.round(share * 100)}%
                    </span>
                  </div>
                  <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-fg/[0.07]">
                    <div className="h-full rounded-full bg-accent" style={{ width: `${Math.max(1, share * 100)}%` }} />
                  </div>
                </li>
              );
            })}
          </ul>
        </Card>
        <Card title="Your golden hour" sub={`You talk most around ${hourLabel(peakHour)}.`}>
          <Bars
            values={stats.hours}
            labels={stats.hours.map((_, h) => hourLabel(h).replace(" ", "").toLowerCase().replace("m", ""))}
            tipLabel={(h) => `${hourLabel(h)} – ${hourLabel((h + 1) % 24)}`}
            tipValue={(v) => `${n(v)} dictations`}
            ariaLabel="Dictations by hour of day"
            tickEvery={6}
          />
          <p className="mt-3 border-t border-line/70 pt-3 text-xs text-muted">
            <span className="font-semibold text-fg">{n(perDictation)}</span> words per dictation on average
          </p>
        </Card>
      </div>

      <div>
        <h2 className="section-title">Personal bests</h2>
        <div className="grid gap-3 [grid-template-columns:repeat(auto-fit,minmax(190px,1fr))]">
          <Best value={n(stats.longest_dictation_words)} title="One thought, fully said" note="words in a single dictation" />
          <Best
            value={n(stats.best_day?.words ?? 0)}
            title="Your biggest day"
            note={stats.best_day ? `words on ${longDate(stats.best_day.date)}` : "words in one day"}
          />
          <Best
            value={n(stats.busiest_day?.dictations ?? 0)}
            title="In the flow"
            note={stats.busiest_day ? `dictations on ${longDate(stats.busiest_day.date)}` : "dictations in one day"}
          />
        </div>
      </div>

      {stats.imported_from_fluidvoice > 0 && (
        <p className="px-1 text-center text-[11px] text-subtle">
          Includes {n(stats.imported_from_fluidvoice)} dictations brought over from FluidVoice.
        </p>
      )}
    </div>
  );
}
