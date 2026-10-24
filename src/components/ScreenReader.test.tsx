import { render, screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import ScreenReader from './ScreenReader';
import { formatHotkey } from '../hooks/useTTS';

const kokoroVoices = [
  { id: 'af_heart', name: 'Heart', language: 'en-US', gender: 'Female' },
  { id: 'am_adam', name: 'Adam', language: 'en-US', gender: 'Male' },
];
const phononVoices = [{ id: 'default', name: 'Phonon', language: 'en-US', gender: 'Neutral' }];

let settings: Record<string, unknown>;
let phononStatus: string;
let speakImpl: () => Promise<unknown>;

beforeEach(() => {
  settings = {
    tts_hotkey: 'Option+S',
    tts_voice: 'af_heart',
    tts_speed: 1.0,
    tts_engine: 'kokoro',
  };
  phononStatus = 'available';
  speakImpl = async () => undefined;
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string, rawArgs?: unknown) => {
    const args = rawArgs as Record<string, unknown> | undefined;
    switch (cmd) {
      case 'get_settings':
        return settings;
      case 'update_settings':
        settings = (args as { settings: Record<string, unknown> }).settings;
        return undefined;
      case 'get_voices':
        return args?.engine === 'phonon' ? phononVoices : kokoroVoices;
      case 'get_model_status':
        return phononStatus;
      case 'speak_text':
        return speakImpl();
      default:
        return undefined;
    }
  });
});

describe('formatHotkey', () => {
  it('turns setting strings into key symbols', () => {
    expect(formatHotkey('Option+S')).toBe('⌥S');
    expect(formatHotkey('CommandOrControl+Shift+S')).toBe('⌘⇧S');
    expect(formatHotkey('Control+Alt+r')).toBe('⌃⌥R');
  });
});

describe('ScreenReader', () => {
  it('shows the configured hotkey, not a hard-coded one', async () => {
    settings.tts_hotkey = 'CommandOrControl+Shift+S';
    render(<ScreenReader />);
    expect(await screen.findByText('⌘⇧S')).toBeInTheDocument();
    expect(screen.getByLabelText('Text to speak')).toHaveAttribute(
      'placeholder',
      expect.stringContaining('⌘⇧S'),
    );
  });

  it('keeps Stop visible until playback finishes', async () => {
    let finish!: () => void;
    speakImpl = () => new Promise<void>((resolve) => (finish = resolve));
    const user = userEvent.setup();
    render(<ScreenReader />);
    await user.type(screen.getByLabelText('Text to speak'), 'Hello there');
    await user.click(screen.getByRole('button', { name: /speak/i }));

    // Old behaviour flipped back to "Speak" as soon as playback started.
    expect(await screen.findByRole('button', { name: /stop/i })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith('speak_text', {
      text: 'Hello there',
      voiceId: 'af_heart',
      speed: 1,
      engine: 'kokoro',
    });

    finish();
    expect(await screen.findByRole('button', { name: /speak/i })).toBeInTheDocument();
  });

  it('shows speech errors', async () => {
    speakImpl = () => Promise.reject('Missing TTS files: kokoro-v1.0.onnx. Please download from Models tab.');
    const user = userEvent.setup();
    render(<ScreenReader />);
    await user.type(screen.getByLabelText('Text to speak'), 'Hi');
    await user.click(screen.getByRole('button', { name: /speak/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Missing TTS files');
  });

  it('switching to Phonon saves the engine and asks for the download', async () => {
    const user = userEvent.setup();
    render(<ScreenReader />);
    await screen.findByText('Adam');
    await user.click(screen.getByRole('radio', { name: /phonon/i }));

    expect(await screen.findByText(/isn't downloaded yet/)).toBeInTheDocument();
    await waitFor(() => expect(settings.tts_engine).toBe('phonon'));
    expect(settings.tts_voice).toBe('default');
    // One voice: no voice grid.
    expect(screen.queryByText('Adam')).not.toBeInTheDocument();

    await user.type(screen.getByLabelText('Text to speak'), 'Hi');
    expect(screen.getByRole('button', { name: /speak/i })).toBeDisabled();
  });

  it('Phonon is usable once downloaded', async () => {
    settings.tts_engine = 'phonon';
    settings.tts_voice = 'default';
    phononStatus = 'downloaded';
    const user = userEvent.setup();
    render(<ScreenReader />);
    await user.type(screen.getByLabelText('Text to speak'), 'Hi');
    await waitFor(() => expect(screen.getByRole('button', { name: /speak/i })).toBeEnabled());
    expect(screen.queryByText(/isn't downloaded yet/)).not.toBeInTheDocument();
  });
});
