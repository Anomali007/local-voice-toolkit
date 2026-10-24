import { render, screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import HistoryPanel, { filterHistory, formatDuration, relativeTime, type HistoryEntry } from './HistoryPanel';

const NOW = 1_700_000_000_000;
const entry = (id: string, text: string, ageMs: number, app: string | null = 'Notes'): HistoryEntry => ({
  id,
  text,
  timestamp_ms: NOW - ageMs,
  duration_ms: 4200,
  app_name: app,
  app_bundle_id: app ? 'com.apple.Notes' : null,
  model: 'ggml-base.en.bin',
});

let store: HistoryEntry[];
let pasteResult: string;

beforeEach(() => {
  store = [entry('b', 'second dictation about Rust', 60_000), entry('a', 'first dictation', 3_600_000, 'Slack')];
  pasteResult = 'pasted';
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string, raw?: unknown) => {
    const args = raw as Record<string, unknown> | undefined;
    switch (cmd) {
      case 'get_history':
        return store;
      case 'get_settings':
        return { history_enabled: true };
      case 'delete_history_entry':
        store = store.filter((e) => e.id !== args!.id);
        return undefined;
      case 'clear_history':
        store = [];
        return undefined;
      case 'paste_history_entry':
        return pasteResult;
      default:
        return undefined;
    }
  });
});

describe('relativeTime', () => {
  it('buckets sensibly', () => {
    expect(relativeTime(NOW - 10_000, NOW)).toBe('just now');
    expect(relativeTime(NOW - 5 * 60_000, NOW)).toBe('5 min ago');
    expect(relativeTime(NOW - 3 * 3_600_000, NOW)).toBe('3 h ago');
    expect(relativeTime(NOW - 30 * 3_600_000, NOW)).toBe('yesterday');
    expect(relativeTime(NOW - 4 * 86_400_000, NOW)).toBe('4 d ago');
    expect(relativeTime(NOW + 5000, NOW)).toBe('just now'); // clock skew
  });
  it('formats durations', () => {
    expect(formatDuration(4200)).toBe('4s');
    expect(formatDuration(75_000)).toBe('1m 15s');
  });
});

describe('filterHistory', () => {
  it('matches text and app name, case-insensitively', () => {
    expect(filterHistory(store, 'RUST').map((e) => e.id)).toEqual(['b']);
    expect(filterHistory(store, 'slack').map((e) => e.id)).toEqual(['a']);
    expect(filterHistory(store, '  ').length).toBe(2);
    expect(filterHistory(store, 'zzz')).toEqual([]);
  });
});

describe('HistoryPanel', () => {
  it('lists entries newest first as given and searches', async () => {
    const user = userEvent.setup();
    render(<HistoryPanel />);
    expect(await screen.findByText('second dictation about Rust')).toBeInTheDocument();
    expect(screen.getByText('first dictation')).toBeInTheDocument();
    await user.type(screen.getByLabelText('Search history'), 'first');
    expect(screen.queryByText('second dictation about Rust')).not.toBeInTheDocument();
    expect(screen.getByText('first dictation')).toBeInTheDocument();
    await user.clear(screen.getByLabelText('Search history'));
    await user.type(screen.getByLabelText('Search history'), 'nothing like this');
    expect(screen.getByText(/Nothing matches/)).toBeInTheDocument();
  });

  it('copies, pastes again and reports what happened', async () => {
    const user = userEvent.setup();
    render(<HistoryPanel />);
    await screen.findByText('first dictation');
    await user.click(screen.getAllByLabelText('Copy dictation')[0]);
    expect(invoke).toHaveBeenCalledWith('copy_history_entry', { id: 'b' });
    expect(await screen.findByText('Copied')).toBeInTheDocument();

    await user.click(screen.getAllByLabelText('Paste dictation again')[1]);
    expect(invoke).toHaveBeenCalledWith('paste_history_entry', { id: 'a' });
    expect(await screen.findByText('Pasted into Slack')).toBeInTheDocument();

    pasteResult = 'copied';
    await user.click(screen.getAllByLabelText('Paste dictation again')[0]);
    expect(await screen.findByText('Copied — press ⌘V')).toBeInTheDocument();
  });

  it('deletes one entry', async () => {
    const user = userEvent.setup();
    render(<HistoryPanel />);
    await screen.findByText('first dictation');
    await user.click(screen.getAllByLabelText('Delete dictation')[0]);
    await waitFor(() => expect(screen.queryByText('second dictation about Rust')).not.toBeInTheDocument());
    expect(screen.getByText('first dictation')).toBeInTheDocument();
  });

  it('clearing needs a second click', async () => {
    const user = userEvent.setup();
    render(<HistoryPanel />);
    await screen.findByText('first dictation');
    await user.click(screen.getByText('Clear history'));
    expect(invoke).not.toHaveBeenCalledWith('clear_history', expect.anything());
    await user.click(screen.getByText('Delete all 2'));
    expect(await screen.findByText(/No dictations yet/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith('clear_history');
  });

  it('says when history is off', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'get_history' ? [] : cmd === 'get_settings' ? { history_enabled: false } : undefined,
    );
    render(<HistoryPanel />);
    expect(await screen.findByText(/History is turned off/)).toBeInTheDocument();
  });
});
