import { render, screen, waitFor } from '@testing-library/react';
import { userEvent } from '@testing-library/user-event';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import DictionaryPanel, { parseTriggers, type DictionaryEntry } from './DictionaryPanel';

let saved: DictionaryEntry[];
let fluid: boolean;

beforeEach(() => {
  saved = [{ id: '1', triggers: ['bto'], replacement: 'BTO' }];
  fluid = true;
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string, raw?: unknown) => {
    const args = raw as Record<string, unknown> | undefined;
    switch (cmd) {
      case 'get_dictionary':
        return saved;
      case 'save_dictionary':
        saved = args!.entries as DictionaryEntry[];
        return saved;
      case 'fluidvoice_dictionary_available':
        return fluid;
      case 'import_fluidvoice_dictionary':
        saved = [...saved, { id: '2', triggers: ['mass lab'], replacement: 'MASS Lab' }];
        return [saved, 1];
      case 'preview_dictionary':
        return `preview:${args!.text}`;
      default:
        return undefined;
    }
  });
});

describe('parseTriggers', () => {
  it('splits on commas and drops blanks', () => {
    expect(parseTriggers(' mass lab, the mass lab ,, ')).toEqual(['mass lab', 'the mass lab']);
  });
});

describe('DictionaryPanel', () => {
  it('lists, adds, edits and deletes words', async () => {
    const user = userEvent.setup();
    render(<DictionaryPanel />);
    expect(await screen.findByText('BTO')).toBeInTheDocument();

    await user.type(screen.getByLabelText('When I say'), 'mass lab, the mass lab');
    await user.type(screen.getByLabelText('Write'), 'MASS Lab');
    await user.click(screen.getByRole('button', { name: 'Add word' }));
    await waitFor(() => expect(saved.map((e) => e.replacement)).toEqual(['BTO', 'MASS Lab']));
    expect(saved[1].triggers).toEqual(['mass lab', 'the mass lab']);

    await user.click(screen.getByRole('button', { name: 'Edit BTO' }));
    const write = screen.getByLabelText('Write');
    await user.clear(write);
    await user.type(write, 'Beat The Odds');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(saved.find((e) => e.id === '1')?.replacement).toBe('Beat The Odds'));

    await user.click(screen.getByRole('button', { name: 'Delete Beat The Odds' }));
    await waitFor(() => expect(saved.map((e) => e.id)).toEqual([expect.any(String)]));
  });

  it('imports from FluidVoice when available', async () => {
    const user = userEvent.setup();
    render(<DictionaryPanel />);
    await user.click(await screen.findByRole('button', { name: 'Import from FluidVoice' }));
    expect(await screen.findByText('Imported 1 entry from FluidVoice')).toBeInTheDocument();
    expect(screen.getByText('MASS Lab')).toBeInTheDocument();
  });

  it('hides import without FluidVoice and previews a sentence', async () => {
    fluid = false;
    const user = userEvent.setup();
    render(<DictionaryPanel />);
    await screen.findByText('BTO');
    expect(screen.queryByRole('button', { name: 'Import from FluidVoice' })).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Try it'), 'i love bto');
    expect(await screen.findByTestId('dict-try-result')).toHaveTextContent('preview:i love bto');
  });
});
