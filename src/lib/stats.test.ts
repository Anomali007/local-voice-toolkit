import { describe, it, expect } from 'vitest';
import {
  addDays,
  compact,
  cumulative,
  dayKey,
  formatMinutes,
  heatmap,
  hourLabel,
  milestone,
  parseDay,
  rangeStart,
  summarizeRange,
  thisWeek,
  weekdayAverages,
  WORD_STEPS,
  type Stats,
} from './stats';

const stats = (days: [string, number, number][], today = '2026-10-24'): Stats => ({
  today,
  total_words: days.reduce((s, d) => s + d[1], 0),
  total_dictations: days.reduce((s, d) => s + d[2], 0),
  total_chars: 0,
  active_days: days.length,
  current_streak: 0,
  best_streak: 0,
  today_words: 0,
  today_dictations: 0,
  days: days.map(([date, words, dictations]) => ({ date, words, dictations })),
  hours: Array(24).fill(0),
  apps: [],
  longest_dictation_words: 0,
  best_day: null,
  busiest_day: null,
  imported_from_fluidvoice: 0,
});

describe('dates', () => {
  it('round-trips and crosses month and DST edges by calendar day', () => {
    expect(dayKey(parseDay('2026-03-08'))).toBe('2026-03-08');
    expect(dayKey(addDays(parseDay('2026-03-07'), 2))).toBe('2026-03-09');
    expect(dayKey(addDays(parseDay('2026-11-01'), -1))).toBe('2026-10-31');
    expect(dayKey(rangeStart(parseDay('2026-10-24'), 1))).toBe('2026-09-25');
  });
});

describe('ranges', () => {
  const s = stats([
    ['2026-01-01', 999, 9], // outside a 1-month range
    ['2026-10-01', 100, 2],
    ['2026-10-20', 300, 3],
    ['2026-10-24', 50, 1],
  ]);

  it('summarizes only the chosen period', () => {
    const r = summarizeRange(s, 1);
    expect(r).toMatchObject({ words: 450, activeDays: 3, perActiveDay: 150, days: 30 });
    expect(r.best?.date).toBe('2026-10-20');
  });

  it('runs a cumulative total from zero', () => {
    const c = cumulative(s, 1);
    expect(c[0].date).toBe('2026-09-25');
    expect(c[c.length - 1]).toEqual({ date: '2026-10-24', words: 50, total: 450 });
    expect(c.length).toBe(30);
  });

  it('averages weekdays including quiet days', () => {
    const avg = weekdayAverages(s, 1);
    // 2026-10-20 is a Tuesday; four or five Tuesdays fall in the range.
    expect(avg[2]).toBeGreaterThan(0);
    expect(avg[2]).toBeLessThan(300);
    expect(avg.reduce((a, b) => a + b, 0)).toBeGreaterThan(0);
  });

  it('lays the heatmap out in Sunday-first weeks with quartile levels', () => {
    const h = heatmap(s, 1);
    expect(h.weeks.every((w) => w.length === 7)).toBe(true);
    const cells = h.weeks.flat();
    expect(parseDay(cells[0].date).getDay()).toBe(0);
    expect(cells.find((c) => c.date === '2026-10-20')?.level).toBe(4);
    expect(cells.find((c) => c.date === '2026-10-24')?.level).toBe(1);
    expect(cells.find((c) => c.date === '2026-10-02')?.level).toBe(0);
    expect(cells.find((c) => c.date === '2026-01-01')).toBeUndefined();
    expect(cells.filter((c) => !c.future).length).toBe(30);
    expect(h.months.length).toBeGreaterThan(0);
  });

  it('shows this week Sunday to Saturday', () => {
    const w = thisWeek(s);
    expect(w.map((d) => d.letter).join('')).toBe('SMTWTFS');
    expect(w[6]).toMatchObject({ date: '2026-10-24', isToday: true, active: true, future: false });
    expect(w[2]).toMatchObject({ date: '2026-10-20', active: true });
  });
});

describe('milestones and formatting', () => {
  it('finds the next milestone', () => {
    expect(milestone(822_160, WORD_STEPS)).toMatchObject({ reached: 5, next: 1_000_000 });
    expect(milestone(822_160, WORD_STEPS).progress).toBeCloseTo(0.822, 3);
    expect(milestone(2_000_000, WORD_STEPS)).toMatchObject({ reached: 6, next: null, progress: 1 });
    expect(milestone(0, WORD_STEPS)).toMatchObject({ reached: 0, next: 1_000 });
  });

  it('formats numbers people read', () => {
    expect(formatMinutes(822_160 / 40)).toBe('342h 34m');
    expect(formatMinutes(45)).toBe('45m');
    expect(compact(999)).toBe('999');
    expect(compact(1_284)).toBe('1.3K');
    expect(compact(400_000)).toBe('400K');
    expect(compact(1_200_000)).toBe('1.2M');
    expect(hourLabel(0)).toBe('12 AM');
    expect(hourLabel(13)).toBe('1 PM');
  });
});
