import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, vi, afterEach } from 'vitest';
import HotkeyRecorder from './HotkeyRecorder';

afterEach(() => vi.restoreAllMocks());

describe('HotkeyRecorder', () => {
  it('does not nest a button inside a button', () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { container } = render(<HotkeyRecorder value="Alt+S" onChange={() => {}} />);
    expect(container.querySelectorAll('button button')).toHaveLength(0);
    expect(screen.getByTitle('Clear hotkey')).toBeInTheDocument();
    const nesting = errorSpy.mock.calls.filter((c) => String(c[0]).includes('validateDOMNesting') || String(c[2] ?? '').includes('button'));
    expect(nesting).toHaveLength(0);
  });

  it('clear button emits an empty hotkey', async () => {
    const onChange = vi.fn();
    render(<HotkeyRecorder value="Alt+S" onChange={onChange} />);
    await userEvent.setup().click(screen.getByTitle('Clear hotkey'));
    expect(onChange).toHaveBeenCalledWith('');
  });
});
