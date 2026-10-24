import { render, screen, fireEvent } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import SettingsPanel from './SettingsPanel';

const base = {
  stt_hotkey: 'RightOption', tts_hotkey: 'Option+S', stt_model: 'ggml-base.en.bin', tts_voice: 'af_heart',
  tts_speed: 1, auto_paste: true, launch_at_login: false, menu_bar_mode: true,
  silence_detection_enabled: true, silence_threshold: 0.01, silence_duration: 1.5, onboarding_completed: true,
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.spyOn(console, 'error').mockImplementation(() => {});
});

describe('SettingsPanel', () => {
  it('shows the error (not a spinner) when get_settings fails', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') throw 'Failed to parse settings file: bad json';
      if (cmd === 'check_permissions') return { microphone: true, accessibility: true };
      return undefined;
    });
    render(<SettingsPanel />);
    expect(await screen.findByText(/Failed to parse settings file: bad json/)).toBeInTheDocument();
    expect(screen.getByText('Retry')).toBeInTheDocument();
  });

  it('does not show the unused Menu bar mode toggle', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return base;
      if (cmd === 'check_permissions') return { microphone: true, accessibility: true };
      return undefined;
    });
    render(<SettingsPanel />);
    await screen.findByText('Keyboard Shortcuts');
    expect(screen.queryByText('Menu bar mode')).not.toBeInTheDocument();
  });

  it('shows the backend error when a hotkey is rejected', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return base;
      if (cmd === 'check_permissions') return { microphone: true, accessibility: true };
      if (cmd === 'update_settings') throw "Read aloud hotkey 'Alt+Nope' is not a valid shortcut";
      return undefined;
    });
    const user = userEvent.setup();
    render(<SettingsPanel />);
    await screen.findByText('Keyboard Shortcuts');
    await user.click(screen.getByTitle('Clear hotkey'));
    expect(await screen.findByRole('alert')).toHaveTextContent(/not a valid shortcut/);
  });
});

describe('SettingsPanel overlay section', () => {
  const overlay = {
    ...base, overlay_position: 'top-center', overlay_size: 'regular', overlay_max_lines: 4,
    overlay_click_through: true, overlay_offset_x: 0, overlay_offset_y: 0,
  };
  const mockBackend = () =>
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return overlay;
      if (cmd === 'check_permissions') return { microphone: true, accessibility: true };
      return undefined;
    });
  const lastSaved = () => {
    const calls = vi.mocked(invoke).mock.calls.filter(([c]) => c === 'update_settings');
    return (calls[calls.length - 1]?.[1] as { settings: Record<string, unknown> }).settings;
  };

  it('shows the saved overlay values', async () => {
    mockBackend();
    render(<SettingsPanel />);
    await screen.findByText('Overlay');
    expect(screen.getByLabelText('Overlay position')).toHaveValue('top-center');
    expect(screen.getByRole('button', { name: 'Regular' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByLabelText('Transcript lines')).toHaveValue('4');
    expect(screen.getByRole('switch', { name: 'Click-through while dictating' })).toHaveAttribute('aria-checked', 'true');
  });

  it('saves position, size, lines and click-through through update_settings', async () => {
    mockBackend();
    const user = userEvent.setup();
    render(<SettingsPanel />);
    await screen.findByText('Overlay');

    await user.selectOptions(screen.getByLabelText('Overlay position'), 'bottom-right');
    await vi.waitFor(() => expect(lastSaved().overlay_position).toBe('bottom-right'));

    await user.click(screen.getByRole('button', { name: 'Large' }));
    await vi.waitFor(() => expect(lastSaved().overlay_size).toBe('large'));

    await user.click(screen.getByRole('switch', { name: 'Click-through while dictating' }));
    await vi.waitFor(() => expect(lastSaved().overlay_click_through).toBe(false));

    fireEvent.change(screen.getByLabelText('Transcript lines'), { target: { value: '7' } });
    await vi.waitFor(() => expect(lastSaved().overlay_max_lines).toBe(7));
    expect(lastSaved().overlay_position).toBe('bottom-right'); // earlier changes persist
  });

  it('ignores a half-typed nudge and saves a real one', async () => {
    mockBackend();
    render(<SettingsPanel />);
    await screen.findByText('Overlay');
    const x = screen.getByLabelText('Overlay nudge X');
    fireEvent.change(x, { target: { value: '' } });
    expect(vi.mocked(invoke).mock.calls.some(([c]) => c === 'update_settings')).toBe(false);
    fireEvent.change(x, { target: { value: '-12' } });
    await vi.waitFor(() => expect(lastSaved().overlay_offset_x).toBe(-12));
  });

  it('Reset reading overlay position invokes the backend reset', async () => {
    mockBackend();
    const user = userEvent.setup();
    render(<SettingsPanel />);
    await screen.findByText('Overlay');
    await user.click(screen.getByRole('button', { name: 'Reset reading overlay position' }));
    expect(invoke).toHaveBeenCalledWith('reset_reading_overlay_position');
    expect(vi.mocked(invoke).mock.calls.some(([c]) => c === 'update_settings')).toBe(false);
  });

  it('Show preview invokes preview_overlay', async () => {
    mockBackend();
    const user = userEvent.setup();
    render(<SettingsPanel />);
    await screen.findByText('Overlay');
    await user.click(screen.getByRole('button', { name: 'Show preview' }));
    expect(invoke).toHaveBeenCalledWith('preview_overlay');
  });
});
