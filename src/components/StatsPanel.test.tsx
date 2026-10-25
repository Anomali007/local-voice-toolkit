import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import StatsPanel from './StatsPanel';
import type { Stats } from '../lib/stats';

const full: Stats = {
  today: '2026-10-24',
  total_words: 822_191,
  total_dictations: 14_677,
  total_chars: 4_260_518,
  active_days: 256,
  current_streak: 106,
  best_streak: 108,
  today_words: 31,
  today_dictations: 1,
  days: [
    { date: '2026-10-22', words: 19_215, dictations: 307 },
    { date: '2026-10-23', words: 2_000, dictations: 40 },
    { date: '2026-10-24', words: 31, dictations: 1 },
  ],
  hours: Array.from({ length: 24 }, (_, h) => (h === 0 ? 962 : 300)),
  apps: [
    { name: 'Terminal', dictations: 5_519, words: 300_000 },
    { name: 'Ghostty', dictations: 4_834, words: 280_000 },
  ],
  longest_dictation_words: 8_222,
  best_day: { date: '2026-10-22', words: 19_215, dictations: 307 },
  busiest_day: { date: '2026-10-22', words: 19_215, dictations: 307 },
  imported_from_fluidvoice: 14_676,
};

let current: Stats;
let fluidVoice: boolean;

beforeEach(() => {
  current = full;
  fluidVoice = true;
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'get_history_stats') return current;
    if (cmd === 'fluidvoice_history_available') return fluidVoice;
    if (cmd === 'import_fluidvoice_history') {
      current = full;
      return { imported: 14_676, already_here: 0, skipped: 0 };
    }
    return undefined;
  });
});

describe('StatsPanel', () => {
  it('shows the headline numbers', async () => {
    render(<StatsPanel />);
    expect(await screen.findByText('106')).toBeInTheDocument();
    expect(screen.getByText('Best: 108 days')).toBeInTheDocument();
    expect(screen.getByText('69 words to today\'s goal')).toBeInTheDocument();
    expect(screen.getAllByText('822,191')).toHaveLength(2); // the all-time tile and the milestone card
    expect(screen.getByText('4,260,518')).toBeInTheDocument();
    expect(screen.getByText('342h 35m')).toBeInTheDocument(); // 822,191 words at 40 wpm
    expect(screen.getByText('8,222')).toBeInTheDocument();
    expect(screen.getByText('Terminal')).toBeInTheDocument();
    expect(screen.getByText('You talk most around 12 AM.')).toBeInTheDocument();
    expect(screen.getByText(/Includes 14,676 dictations brought over from FluidVoice/)).toBeInTheDocument();
    // Already imported: no import offer.
    expect(screen.queryByText('Bring your FluidVoice history')).not.toBeInTheDocument();
  });

  it('shows details when a day is hovered', async () => {
    const user = userEvent.setup();
    render(<StatsPanel />);
    const cell = await screen.findByLabelText(/Oct 22, 2026: 19,215 words/);
    await user.hover(cell);
    expect(await screen.findByRole('tooltip')).toHaveTextContent('19,215 words');
  });

  it('offers the FluidVoice import on an empty history', async () => {
    current = { ...full, total_words: 0, total_dictations: 0, days: [], apps: [], imported_from_fluidvoice: 0 };
    const user = userEvent.setup();
    render(<StatsPanel />);
    await user.click(await screen.findByText('Import'));
    expect(invoke).toHaveBeenCalledWith('import_fluidvoice_history');
    expect(await screen.findByText('Brought over 14,676 dictations from FluidVoice.')).toBeInTheDocument();
    expect(await screen.findByText('106')).toBeInTheDocument();
  });

  it('has a plain empty state without FluidVoice', async () => {
    current = { ...full, total_words: 0, total_dictations: 0, days: [], apps: [], imported_from_fluidvoice: 0 };
    fluidVoice = false;
    render(<StatsPanel />);
    expect(await screen.findByText(/No dictations yet/)).toBeInTheDocument();
    expect(screen.queryByText('Import')).not.toBeInTheDocument();
  });
});
