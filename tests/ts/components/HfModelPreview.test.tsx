/**
 * The HuggingFace preview's projector note: under the quantization table,
 * the projector the selected quantization's download fetches — its file
 * name, its size, what it costs — and nothing for a repository without one.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import type { HfModelSummary, HfQuantization } from '../../../src/types';

const MIB = 1024 * 1024;

const listing: { current: HfQuantization[] } = { current: [] };
const checkFit = vi.fn((_bytes: number) => 'fits' as const);

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  openUrl: vi.fn(),
}));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    getHfQuantizations: (modelId: string) =>
      Promise.resolve({ model_id: modelId, quantizations: listing.current }),
    getHfToolSupport: () =>
      Promise.resolve({ supports_tool_calls: false, confidence: 0, detected_format: null }),
  }),
}));
vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({ settings: null }),
}));
vi.mock('../../../src/hooks/useSystemMemory', () => ({
  useSystemMemory: () => ({ checkFit, getTooltip: () => 'fit', loading: false }),
}));

import { HfModelPreview } from '../../../src/components/HfModelPreview';

const MODEL: HfModelSummary = {
  id: 'owner/X-GGUF',
  name: 'X-GGUF',
  author: 'owner',
  downloads: 0,
  likes: 0,
  last_modified: null,
  parameters_b: null,
  description: null,
  tags: [],
};

function quant(name: string, sizeMib: number, projector: HfQuantization['projector']): HfQuantization {
  return {
    name,
    file_path: `X-${name}.gguf`,
    size_bytes: sizeMib * MIB,
    size_mb: sizeMib,
    is_sharded: false,
    shard_count: null,
    projector,
  };
}

const F16 = { file_path: 'mmproj-F16.gguf', size_bytes: 880 * MIB };
const Q8 = { file_path: 'sub/X.mmproj-Q8_0.gguf', size_bytes: 629 * MIB };

async function preview(quantizations: HfQuantization[]) {
  listing.current = quantizations;
  const onDownload = vi.fn();
  render(<HfModelPreview model={MODEL} onDownload={onDownload} />);
  await screen.findByText('Quant');
  return { onDownload, user: userEvent.setup() };
}

describe('HfModelPreview — the projector that comes with a download', () => {
  beforeEach(() => {
    checkFit.mockClear();
  });

  it('shows the first quantization\'s projector: its file name, its size and its cost', async () => {
    await preview([quant('Q4_K_M', 16000, F16), quant('Q8_0', 28000, Q8)]);

    const note = screen.getByTestId('projector-note');
    expect(note).toHaveTextContent('Q4_K_M comes with a projector');
    expect(within(note).getByText('mmproj-F16.gguf')).toBeInTheDocument();
    // The size is formatted as the table's sizes are.
    expect(within(note).getByText('880 MiB')).toBeInTheDocument();
    expect(note).toHaveTextContent('It adds about 880 MiB in memory.');
    expect(note).toHaveTextContent('context shift and cache reuse by shifting are off');
    expect(screen.getByTestId('quant-row-Q4_K_M')).toHaveAttribute('aria-current', 'true');
  });

  it('changes with the selection when the selected quantization fetches another projector', async () => {
    const { user } = await preview([quant('Q4_K_M', 16000, F16), quant('Q8_0', 28000, Q8)]);

    await user.click(screen.getByText('Q8_0'));

    const note = screen.getByTestId('projector-note');
    expect(note).toHaveTextContent('Q8_0 comes with a projector');
    // The file's name, without its directory in the repository.
    expect(within(note).getByText('X.mmproj-Q8_0.gguf')).toBeInTheDocument();
    expect(within(note).getByText('629 MiB')).toBeInTheDocument();
    expect(within(note).queryByText('mmproj-F16.gguf')).not.toBeInTheDocument();
    expect(screen.getByTestId('quant-row-Q8_0')).toHaveAttribute('aria-current', 'true');
    expect(screen.getByTestId('quant-row-Q4_K_M')).not.toHaveAttribute('aria-current');
  });

  it('selects a row when the keyboard reaches its download button', async () => {
    const { user } = await preview([quant('Q4_K_M', 16000, F16), quant('Q8_0', 28000, Q8)]);

    const download = (row: string) =>
      within(screen.getByTestId(`quant-row-${row}`)).getByRole('button', { name: 'Download' });
    act(() => download('Q4_K_M').focus());
    await user.tab();

    expect(download('Q8_0')).toHaveFocus();
    expect(await screen.findByText('X.mmproj-Q8_0.gguf')).toBeInTheDocument();
  });

  it('shows nothing for a repository with no projector', async () => {
    const { user } = await preview([quant('Q4_K_M', 16000, null), quant('Q8_0', 28000, null)]);

    expect(screen.queryByTestId('projector-note')).not.toBeInTheDocument();
    await user.click(screen.getByText('Q8_0'));
    expect(screen.queryByTestId('projector-note')).not.toBeInTheDocument();
  });

  it('downloads the row\'s quantization, and shows the weights\' size alone', async () => {
    const { user, onDownload } = await preview([quant('Q4_K_M', 16000, F16), quant('Q8_0', 28000, Q8)]);

    const row = screen.getByTestId('quant-row-Q8_0');
    expect(within(row).getByText('27.34 GiB')).toBeInTheDocument();
    await user.click(within(row).getByRole('button', { name: 'Download' }));

    expect(onDownload).toHaveBeenCalledWith('owner/X-GGUF', 'Q8_0');
  });

  it('judges the fit with the projector counted, since both are loaded', async () => {
    await preview([quant('Q8_0', 28000, Q8), quant('Q9', 30000, null)]);

    expect(checkFit).toHaveBeenCalledWith((28000 + 629) * MIB);
    expect(checkFit).toHaveBeenCalledWith(30000 * MIB);
  });
});
