import { render, screen, act, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { userEvent } from '@testing-library/user-event';
import DictationOverlay, { liveTail, tailChars } from './DictationOverlay';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { formatClock, formatTimes, stepSpeed } from './ReadingOverlay';

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Handler>();

beforeEach(() => {
  handlers.clear();
  vi.mocked(listen).mockImplementation(async (name: string, handler: unknown) => {
    handlers.set(name, handler as Handler);
    return () => {};
  });
  // jsdom has no canvas
  HTMLCanvasElement.prototype.getContext = vi.fn(() => null) as never;
});

const fire = (name: string, payload: unknown = null) =>
  act(() => handlers.get(name)?.({ payload }));

describe('liveTail', () => {
  it('keeps short text and trims long text at a word boundary', () => {
    expect(liveTail('hello world')).toBe('hello world');
    const long = 'word '.repeat(100).trim();
    const tail = liveTail(long, 50);
    expect(tail.startsWith('…')).toBe(true);
    expect(tail.length).toBeLessThanOrEqual(51);
    expect(long.endsWith(tail.slice(1).trim())).toBe(true);
  });
});

describe('DictationOverlay', () => {
  it('shows words while still recording', async () => {
    render(<DictationOverlay />);
    await vi.waitFor(() => expect(handlers.has('stt-partial-result')).toBe(true));
    fire('stt-recording-started', { target_app: { name: 'Notes', bundle_id: 'com.apple.Notes' } });
    expect(screen.getByText(/tap the hotkey again to finish and paste/)).toBeInTheDocument();

    fire('stt-partial-result', 'hello this is a live');
    expect(screen.getByText('Recording')).toBeInTheDocument();
    expect(screen.getByText('hello this is a live')).toBeInTheDocument();

    fire('stt-result', 'Hello, this is a live test.');
    expect(screen.getByText('Hello, this is a live test.')).toBeInTheDocument();
  });

  it('a new recording clears the previous live text', async () => {
    render(<DictationOverlay />);
    await vi.waitFor(() => expect(handlers.has('stt-partial-result')).toBe(true));
    fire('stt-recording-started', { target_app: null });
    fire('stt-partial-result', 'old words');
    fire('stt-recording-started', { target_app: null });
    expect(screen.queryByText('old words')).not.toBeInTheDocument();
  });
});

describe('DictationOverlay reading mode', () => {
  const settings = { overlay_size: 'regular', overlay_max_lines: 4, tts_speed: 1.3 };
  const progress = (over: Record<string, unknown> = {}) => ({
    elapsed_s: 12, total_s: 65, total_estimated: true, speed: 1.3, chunk_index: 2, chunk_count: 5, ...over,
  });

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settings;
      if (cmd === 'set_tts_speed') return 1.4;
      return undefined;
    });
  });

  const startReading = async () => {
    render(<DictationOverlay />);
    await vi.waitFor(() => expect(handlers.has('tts-progress')).toBe(true));
    fire('tts-reading', true);
    await act(async () => {}); // let the settings read settle
  };

  it('switches to the compact pill and shows one "elapsed / total" time', async () => {
    await startReading();
    expect(screen.getByTestId('reading-pill')).toBeInTheDocument();
    expect(screen.getByText('Preparing…')).toBeInTheDocument();
    expect(screen.queryByText('Reading aloud')).not.toBeInTheDocument();

    fire('tts-progress', progress());
    expect(screen.getByText('0:12 / ~1:05')).toBeInTheDocument(); // estimated total
    expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '18');
    expect(screen.getByText('1.3×')).toBeInTheDocument();
    expect(screen.queryByText(/Recording/)).not.toBeInTheDocument();

    fire('tts-progress', progress({ elapsed_s: 42, total_s: 130, total_estimated: false }));
    expect(screen.getByText('0:42 / 2:10')).toBeInTheDocument();
  });

  it('dragging the pill starts a native window drag, but not from a button', async () => {
    const startDragging = vi.fn(async () => undefined);
    vi.mocked(getCurrentWindow).mockReturnValue({ startDragging } as never);
    await startReading();
    fireEvent.mouseDown(screen.getByTestId('reading-pill'), { button: 0 });
    expect(startDragging).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Faster' }), { button: 0 });
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Stop' }), { button: 0 });
    fireEvent.mouseDown(screen.getByTestId('reading-pill'), { button: 2 });
    expect(startDragging).toHaveBeenCalledTimes(1);
  });

  it('+ and − call set_tts_speed with the next step', async () => {
    const user = userEvent.setup();
    await startReading();
    fire('tts-progress', progress());

    await user.click(screen.getByRole('button', { name: 'Faster' }));
    expect(invoke).toHaveBeenCalledWith('set_tts_speed', { speed: 1.4 });
    await vi.waitFor(() => expect(screen.getByText('1.4×')).toBeInTheDocument());

    await user.click(screen.getByRole('button', { name: 'Slower' }));
    expect(invoke).toHaveBeenCalledWith('set_tts_speed', { speed: 1.3 });
  });

  it('Stop invokes stop_speaking', async () => {
    const user = userEvent.setup();
    await startReading();
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(invoke).toHaveBeenCalledWith('stop_speaking');
  });

  it('speed stops at the supported range', async () => {
    await startReading();
    fire('tts-progress', progress({ speed: 3.0 }));
    expect(screen.getByRole('button', { name: 'Faster' })).toBeDisabled();
    fire('tts-progress', progress({ speed: 0.5 }));
    expect(screen.getByRole('button', { name: 'Slower' })).toBeDisabled();
  });

  it('returns to the dictation face when reading ends or dictation starts', async () => {
    await startReading();
    fire('tts-reading', false);
    expect(screen.queryByTestId('reading-pill')).not.toBeInTheDocument();
    fire('tts-reading', true);
    expect(screen.getByTestId('reading-pill')).toBeInTheDocument();
    fire('stt-recording-started', { target_app: null });
    await act(async () => {});
    expect(screen.queryByTestId('reading-pill')).not.toBeInTheDocument();
  });
});

describe('ReadingOverlay helpers', () => {
  it('formats clocks and steps speed on the 0.1 grid', () => {
    expect(formatClock(75)).toBe('1:15');
    expect(formatClock(-3)).toBe('0:00');
    expect(stepSpeed(1.3, 1)).toBe(1.4);
    expect(stepSpeed(1.1, -1)).toBe(1);
    expect(stepSpeed(3, 1)).toBe(3);
    expect(stepSpeed(0.5, -1)).toBe(0.5);
    expect(formatTimes(null)).toBe('Preparing…');
  });
});

describe('DictationOverlay line setting', () => {
  it('clamps the transcript to the configured number of lines', async () => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === 'get_settings' ? { overlay_size: 'large', overlay_max_lines: 7, tts_speed: 1 } : undefined);
    render(<DictationOverlay />);
    await vi.waitFor(() => expect(handlers.has('stt-partial-result')).toBe(true));
    fire('stt-recording-started', { target_app: null });
    fire('stt-partial-result', 'some live words');
    await act(async () => {});
    await vi.waitFor(() => expect(screen.getByText('some live words')).toHaveStyle({ WebkitLineClamp: '7' }));
    expect(tailChars(7)).toBeGreaterThan(tailChars(4));
  });
});
