/** Types for `get_history_stats` and the date maths behind the Stats page. */

export interface DayStat {
  /** Local date, "YYYY-MM-DD". */
  date: string;
  words: number;
  dictations: number;
}

export interface AppStat {
  name: string;
  dictations: number;
  words: number;
}

export interface Stats {
  today: string;
  total_words: number;
  total_dictations: number;
  total_chars: number;
  active_days: number;
  current_streak: number;
  best_streak: number;
  today_words: number;
  today_dictations: number;
  days: DayStat[];
  hours: number[];
  apps: AppStat[];
  longest_dictation_words: number;
  best_day: DayStat | null;
  busiest_day: DayStat | null;
  imported_from_fluidvoice: number;
}

/** Typing speed the "time saved" figure is measured against. */
export const TYPING_WPM = 40;
/** Words that count as showing up today. */
export const DAILY_GOAL = 100;

export const RANGES = [1, 3, 6, 12] as const;
export type Range = (typeof RANGES)[number];

export const WORD_STEPS = [1_000, 10_000, 50_000, 100_000, 500_000, 1_000_000];
export const DICTATION_STEPS = [10, 100, 500, 1_000, 5_000, 10_000];
export const STREAK_STEPS = [3, 7, 14, 30, 100, 365];

/** "2026-10-24" -> local midnight. */
export function parseDay(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function dayKey(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Calendar days, safe across daylight-saving changes. */
export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}

/** The first day of a range that ends today. */
export function rangeStart(today: Date, months: number): Date {
  return addDays(new Date(today.getFullYear(), today.getMonth() - months, today.getDate()), 1);
}

function eachDay(from: Date, to: Date, fn: (d: Date) => void) {
  for (let d = from; d <= to; d = addDays(d, 1)) fn(d);
}

function byDate(stats: Stats): Map<string, DayStat> {
  return new Map(stats.days.map((d) => [d.date, d]));
}

export interface RangeSummary {
  words: number;
  days: number;
  activeDays: number;
  best: DayStat | null;
  perActiveDay: number;
}

export function summarizeRange(stats: Stats, months: number): RangeSummary {
  const today = parseDay(stats.today);
  const from = dayKey(rangeStart(today, months));
  const inRange = stats.days.filter((d) => d.date >= from && d.date <= stats.today);
  const words = inRange.reduce((s, d) => s + d.words, 0);
  const best = inRange.reduce<DayStat | null>((b, d) => (!b || d.words > b.words ? d : b), null);
  const days = Math.round((today.getTime() - parseDay(from).getTime()) / 86_400_000) + 1;
  return {
    words,
    days,
    activeDays: inRange.length,
    best,
    perActiveDay: inRange.length ? Math.round(words / inRange.length) : 0,
  };
}

export interface HeatCell {
  date: string;
  words: number;
  dictations: number;
  /** 0 = quiet, 1-4 = quartiles of the active days shown. */
  level: number;
  future: boolean;
}

export interface Heatmap {
  /** Columns are weeks (Sunday first); each has 7 cells. */
  weeks: HeatCell[][];
  months: { col: number; label: string }[];
}

export function heatmap(stats: Stats, months: number): Heatmap {
  const today = parseDay(stats.today);
  const first = rangeStart(today, months);
  const start = addDays(first, -first.getDay());
  const end = addDays(today, 6 - today.getDay());
  const lookup = byDate(stats);
  const firstKey = dayKey(first);

  const active = stats.days
    .filter((d) => d.date >= firstKey && d.date <= stats.today)
    .map((d) => d.words)
    .sort((a, b) => a - b);
  const q = (p: number) => active[Math.floor(p * (active.length - 1))] ?? 0;
  const [q1, q2, q3] = [q(0.25), q(0.5), q(0.75)];
  const top = active[active.length - 1] ?? 0;
  // The busiest day always gets the darkest shade.
  const level = (w: number) => (w <= 0 ? 0 : w >= top ? 4 : w <= q1 ? 1 : w <= q2 ? 2 : w <= q3 ? 3 : 4);

  const weeks: HeatCell[][] = [];
  const monthsOut: { col: number; label: string }[] = [];
  eachDay(start, end, (d) => {
    if (d.getDay() === 0) weeks.push([]);
    const key = dayKey(d);
    const inRange = key >= firstKey && key <= stats.today;
    const day = inRange ? lookup.get(key) : undefined;
    weeks[weeks.length - 1].push({
      date: key,
      words: day?.words ?? 0,
      dictations: day?.dictations ?? 0,
      level: level(day?.words ?? 0),
      future: !inRange,
    });
    if (d.getDate() === 1 || (weeks.length === 1 && d.getDay() === 0)) {
      const col = weeks.length - 1;
      const label = (d.getDate() === 1 ? d : addDays(d, 6)).toLocaleDateString(undefined, { month: "short" });
      const last = monthsOut[monthsOut.length - 1];
      if (!last) monthsOut.push({ col, label });
      else if (col - last.col >= 3 && last.label !== label) monthsOut.push({ col, label });
    }
  });
  return { weeks, months: monthsOut };
}

export interface CumulativePoint {
  date: string;
  words: number;
  total: number;
}

/** Running total of words across the range, one point per day. */
export function cumulative(stats: Stats, months: number): CumulativePoint[] {
  const today = parseDay(stats.today);
  const lookup = byDate(stats);
  const out: CumulativePoint[] = [];
  let total = 0;
  eachDay(rangeStart(today, months), today, (d) => {
    const words = lookup.get(dayKey(d))?.words ?? 0;
    total += words;
    out.push({ date: dayKey(d), words, total });
  });
  return out;
}

/** Average words per weekday (Sunday first), counting quiet days as zero. */
export function weekdayAverages(stats: Stats, months: number): number[] {
  const today = parseDay(stats.today);
  const lookup = byDate(stats);
  const sum = Array(7).fill(0);
  const count = Array(7).fill(0);
  eachDay(rangeStart(today, months), today, (d) => {
    sum[d.getDay()] += lookup.get(dayKey(d))?.words ?? 0;
    count[d.getDay()] += 1;
  });
  return sum.map((s, i) => (count[i] ? Math.round(s / count[i]) : 0));
}

export interface WeekDay {
  date: string;
  letter: string;
  active: boolean;
  isToday: boolean;
  future: boolean;
}

/** This week, Sunday to Saturday. */
export function thisWeek(stats: Stats): WeekDay[] {
  const today = parseDay(stats.today);
  const lookup = byDate(stats);
  const sunday = addDays(today, -today.getDay());
  return Array.from({ length: 7 }, (_, i) => {
    const d = addDays(sunday, i);
    const key = dayKey(d);
    return {
      date: key,
      letter: "SMTWTFS"[i],
      active: lookup.has(key),
      isToday: key === stats.today,
      future: key > stats.today,
    };
  });
}

export interface Milestone {
  reached: number;
  steps: number;
  /** The next step, or null when every step is reached. */
  next: number | null;
  /** 0-1 progress toward `next` (1 when done). */
  progress: number;
}

export function milestone(value: number, steps: number[]): Milestone {
  const reached = steps.filter((s) => value >= s).length;
  const next = steps[reached] ?? null;
  return { reached, steps: steps.length, next, progress: next ? Math.min(1, value / next) : 1 };
}

/** 15072 minutes -> "251h 12m"; 45 -> "45m". */
export function formatMinutes(minutes: number): string {
  const m = Math.round(minutes);
  const h = Math.floor(m / 60);
  return h ? `${h.toLocaleString()}h ${m % 60}m` : `${m}m`;
}

/** 1284 -> "1.3K", 822160 -> "822K", 1200000 -> "1.2M". */
export function compact(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${+(n / 1000).toFixed(n < 10_000 ? 1 : 0)}K`;
  return `${+(n / 1_000_000).toFixed(1)}M`;
}

/** 0 -> "12 AM", 13 -> "1 PM". */
export function hourLabel(h: number): string {
  const suffix = h < 12 ? "AM" : "PM";
  return `${h % 12 === 0 ? 12 : h % 12} ${suffix}`;
}

export function longDate(s: string): string {
  return parseDay(s).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: "numeric" });
}
