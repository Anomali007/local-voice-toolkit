import { render, screen, act, fireEvent } from '@testing-library/react';
import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import Splash from './Splash';

function mockMatchMedia(reduce: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: reduce,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  });
}

describe('Splash', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mockMatchMedia(false);
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders, runs through its phases, then calls onDone once', () => {
    const onDone = vi.fn();
    render(<Splash onDone={onDone} />);
    const splash = screen.getByTestId('splash');
    expect(splash).toHaveAttribute('data-phase', 'talk');
    expect(screen.getAllByText('blah')).toHaveLength(3);
    expect(onDone).not.toHaveBeenCalled();

    act(() => { vi.advanceTimersByTime(1300); });
    expect(splash).toHaveAttribute('data-phase', 'merge');
    expect(screen.getByRole('img', { name: 'Blah³' })).toBeInTheDocument();

    act(() => { vi.advanceTimersByTime(1000); });
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('skips on click', () => {
    const onDone = vi.fn();
    render(<Splash onDone={onDone} />);
    fireEvent.click(screen.getByTestId('splash'));
    expect(onDone).toHaveBeenCalledTimes(1);
    act(() => { vi.advanceTimersByTime(3000); });
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('skips on any key', () => {
    const onDone = vi.fn();
    render(<Splash onDone={onDone} />);
    fireEvent.keyDown(window, { key: 'a' });
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it('is skipped under prefers-reduced-motion', () => {
    mockMatchMedia(true);
    const onDone = vi.fn();
    render(<Splash onDone={onDone} />);
    expect(screen.queryByTestId('splash')).not.toBeInTheDocument();
    expect(onDone).toHaveBeenCalledTimes(1);
  });
});
