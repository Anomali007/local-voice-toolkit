import { render, screen } from '@testing-library/react';
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
