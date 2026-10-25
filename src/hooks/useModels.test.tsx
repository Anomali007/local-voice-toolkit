import { renderHook, act, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useModels } from './useModels';

const settings = { stt_model: 'ggml-base.en.bin', tts_hotkey: 'Alt+S' };

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe('useModels', () => {
  it('surfaces download errors per model', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'download_model') throw 'network down';
      if (cmd === 'get_settings') return settings;
      return [];
    });
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const { result } = renderHook(() => useModels());
    await act(async () => {
      await result.current.downloadModel('ggml-small.en.bin');
    });
    expect(result.current.errors['ggml-small.en.bin']).toContain('network down');
    expect(result.current.isDownloading('ggml-small.en.bin')).toBe(false);
  });

  it('selectSttModel persists stt_model and marks it active', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settings;
      return [];
    });
    const { result } = renderHook(() => useModels());
    await waitFor(() => expect(result.current.activeSttModel).toBe('ggml-base.en.bin'));
    await act(async () => {
      await result.current.selectSttModel('ggml-small.en.bin');
    });
    expect(invoke).toHaveBeenCalledWith('update_settings', {
      settings: { ...settings, stt_model: 'ggml-small.en.bin' },
    });
    expect(result.current.activeSttModel).toBe('ggml-small.en.bin');
  });

  it('reports a failed model switch and keeps the old active model', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settings;
      if (cmd === 'update_settings') throw 'disk full';
      return [];
    });
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const { result } = renderHook(() => useModels());
    await waitFor(() => expect(result.current.activeSttModel).toBe('ggml-base.en.bin'));
    await act(async () => {
      await result.current.selectSttModel('ggml-small.en.bin');
    });
    expect(result.current.errors['ggml-small.en.bin']).toContain('disk full');
    expect(result.current.activeSttModel).toBe('ggml-base.en.bin');
  });
});
