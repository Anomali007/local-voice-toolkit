import { render, screen, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { listen } from '@tauri-apps/api/event';
import DictationOverlay, { liveTail } from './DictationOverlay';

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
