/**
 * The paired machine's group in the library: headed by its name and whether
 * it is reached, one row per model badged with the machine, the library's
 * search applied, and set aside with a note while a filter (which describes
 * this machine's models) is on.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { PairedModelRows } from '../../../src/components/ModelLibraryPanel/PairedModelRows';
import type { PairedModelsState, PairedReach } from '../../../src/hooks/usePairedModels';
import { FAR_FINGERPRINT, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

function paired(reach: PairedReach = 'reached'): PairedModelsState {
  const group = pairedModels([farEntry('qwen3', 3), farEntry('llama-3.1', 1000, { description: 'llama - 8B' })]);
  return { group, name: 'desk', reach, refetch: vi.fn() };
}

function rows(props: Partial<Parameters<typeof PairedModelRows>[0]> = {}) {
  const onPick = vi.fn();
  render(
    <PairedModelRows paired={paired()} searchQuery="" filtersActive={false} picked={null} onPick={onPick} {...props} />,
  );
  return onPick;
}

describe('PairedModelRows', () => {
  it('heads the group with the machine and badges each row with it', () => {
    rows();
    const group = screen.getByRole('region', { name: "desk's models" });
    expect(within(group).getByText('reached')).toBeInTheDocument();
    const options = within(group).getAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining('qwen3desk'),
      expect.stringContaining('llama-3.1desk'),
    ]);
  });

  it('marks a row whose model reads images, from the capabilities its machine lists', () => {
    const group = pairedModels([
      farEntry('qwen3-vl', 3, { capabilities: ['vision'] }),
      farEntry('bge', 4, { capabilities: ['embeddings'] }),
      farEntry('llama-3.1', 1000),
    ]);
    rows({ paired: { group, name: 'desk', reach: 'reached', refetch: vi.fn() } });
    const marked = screen.getAllByRole('option').map((o) => within(o).queryByText('Vision') !== null);
    expect(marked).toEqual([true, false, false]);
  });

  it('picks a row as that machine’s model, by its id there', async () => {
    const onPick = rows();
    await userEvent.setup().click(screen.getByRole('option', { name: /llama-3.1/ }));
    expect(onPick).toHaveBeenCalledWith({ machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 1000 });
  });

  it('marks the picked row', () => {
    rows({ picked: { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 } });
    expect(screen.getByRole('option', { name: /qwen3/ })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('option', { name: /llama/ })).toHaveAttribute('aria-selected', 'false');
  });

  it('matches the library’s search', () => {
    rows({ searchQuery: 'LLAMA' });
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([expect.stringContaining('llama-3.1')]);
  });

  it('is set aside with a note while a filter is on', () => {
    rows({ filtersActive: true });
    expect(screen.queryByRole('option')).not.toBeInTheDocument();
    expect(screen.getByText("Filters describe this machine's models; clear them to see desk's.")).toBeInTheDocument();
  });

  it.each(['away', 'stale'] as const)('says the rows are %s', (reach) => {
    rows({ paired: paired(reach) });
    expect(screen.getByText(reach)).toBeInTheDocument();
    expect(screen.getAllByRole('option')).toHaveLength(2);
  });

  it('shows nothing before that machine’s models are read', () => {
    const { container } = render(
      <PairedModelRows
        paired={{ group: null, name: 'desk', reach: 'reached', refetch: vi.fn() }}
        searchQuery=""
        filtersActive={false}
        picked={null}
        onPick={vi.fn()}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
