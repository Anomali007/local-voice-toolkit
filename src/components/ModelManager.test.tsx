import { render, screen } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import ModelManager from './ModelManager';

const model = (id: string, name: string, status: string) => ({
  id, name, model_type: 'stt', size_bytes: 1, size_display: '1 MB', download_url: '', status, description: 'd',
});

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'list_models')
      return [model('ggml-base.en.bin', 'Base', 'downloaded'), model('ggml-small.en.bin', 'Small', 'downloaded')];
    if (cmd === 'get_settings') return { stt_model: 'ggml-base.en.bin' };
    return undefined;
  });
});

describe('ModelManager', () => {
  it('marks the active model and offers "Use this model" on the other', async () => {
    render(<ModelManager />);
    expect(await screen.findByText('Active')).toBeInTheDocument();
    expect(screen.getAllByText('Use this model')).toHaveLength(1);
  });

  it('requires confirmation before delete and warns for the active model', async () => {
    const user = userEvent.setup();
    render(<ModelManager />);
    const deletes = await screen.findAllByText('Delete');
    await user.click(deletes[0]); // Base = active
    expect(screen.getByText(/active dictation model/i)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith('delete_model', expect.anything());
    await user.click(screen.getByText('Confirm delete'));
    expect(invoke).toHaveBeenCalledWith('delete_model', { modelId: 'ggml-base.en.bin' });
  });

  it('cancel does not delete', async () => {
    const user = userEvent.setup();
    render(<ModelManager />);
    await user.click((await screen.findAllByText('Delete'))[0]);
    await user.click(screen.getByText('Cancel'));
    expect(invoke).not.toHaveBeenCalledWith('delete_model', expect.anything());
  });
});
