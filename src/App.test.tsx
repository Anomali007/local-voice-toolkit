import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import App from './App';

const mockSettings = {
  stt_hotkey: 'RightOption',
  tts_hotkey: 'Option+S',
  stt_model: 'ggml-base.en.bin',
  tts_voice: 'af_heart',
  tts_speed: 1.0,
  auto_paste: true,
  launch_at_login: false,
  menu_bar_mode: true,
  silence_detection_enabled: true,
  silence_threshold: 0.01,
  silence_duration: 1.5,
  onboarding_completed: true,
};

beforeEach(() => {
  vi.mocked(invoke).mockImplementation(async (cmd: string): Promise<unknown> => {
    switch (cmd) {
      case 'get_settings':
        return mockSettings;
      case 'get_hardware_info':
        return {
          chip: 'apple_silicon',
          chip_name: 'Apple M3',
          ram_gb: 16,
          cpu_cores: 8,
          has_neural_engine: true,
          has_metal: true,
          recommended_tier: 'balanced',
        };
      case 'check_permissions':
        return { microphone: true, accessibility: true };
      case 'list_models':
        return [];
      case 'get_voices':
        return [];
      default:
        return undefined;
    }
  });
});

describe('App', () => {
  it('renders the header with Blah³ title', async () => {
    render(<App />);
    expect(await screen.findByText('Blah³')).toBeInTheDocument();
  });

  it('renders all navigation tabs', async () => {
    render(<App />);
    expect(await screen.findByText('Dictation')).toBeInTheDocument();
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('Models')).toBeInTheDocument();
    expect(screen.getByText('Settings')).toBeInTheDocument();
  });

  it('shows Dictation panel by default', async () => {
    render(<App />);
    // The Dictation tab should be active (has sky-400 color class)
    const dictationTab = (await screen.findByText('Dictation')).closest('button');
    expect(dictationTab).toHaveClass('text-sky-400');
  });

  it('shows onboarding when not completed', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string): Promise<unknown> => {
      if (cmd === 'get_settings') return { ...mockSettings, onboarding_completed: false };
      if (cmd === 'check_permissions') return { microphone: false, accessibility: false };
      if (cmd === 'list_models') return [];
      return undefined;
    });
    render(<App />);
    // Onboarding replaces the main app shell — no tab navigation visible
    await screen.findAllByText(/Blah/);
    expect(screen.queryByText('Reader')).not.toBeInTheDocument();
  });

  it('switches tabs when clicked', async () => {
    const user = userEvent.setup();
    render(<App />);

    // Click on Settings tab
    await user.click(await screen.findByText('Settings'));

    // Settings tab should now be active
    const settingsTab = screen.getByText('Settings').closest('button');
    expect(settingsTab).toHaveClass('text-sky-400');

    // Dictation tab should no longer be active
    const dictationTab = screen.getByText('Dictation').closest('button');
    expect(dictationTab).not.toHaveClass('text-sky-400');
  });
});
