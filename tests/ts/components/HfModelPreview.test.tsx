/**
 * The HuggingFace preview's projector note: under the quantization table,
 * the projector the selected quantization's download fetches — its file
 * name, its size, what it costs — and nothing for a repository without one.
 *
 * And its companion note: for a repository the daemon reads as an image
 * model, the family, each companion's role, name, repository and size,
 * "already here" for one already in the models directory, and the bytes the
 * companions add to a download; nothing for any other repository.
 */

import { describe, it, expect, vi } from 'vitest';
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import type { HfImagePreview, HfModelSummary, HfQuantization } from '../../../src/types';

const MIB = 1024 * 1024;

const listing: { current: HfQuantization[] } = { current: [] };
const imageListing: { current: HfImagePreview | null } = { current: null };
/** A repository whose listing never answers, as one still loading. */
const STILL_LOADING = 'owner/Still-Loading-GGUF';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
  openUrl: vi.fn(),
}));
vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    getHfQuantizations: (modelId: string) =>
      modelId === STILL_LOADING
        ? new Promise(() => {})
        : Promise.resolve({ model_id: modelId, quantizations: listing.current, image: imageListing.current }),
    getHfToolSupport: () =>
      Promise.resolve({ supports_tool_calls: false, confidence: 0, detected_format: null }),
  }),
}));
vi.mock('../../../src/hooks/useSettings', () => ({
  useSettings: () => ({ settings: null }),
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

function quant(
  label: string,
  sizeMib: number,
  projector: HfQuantization['projector'],
  role: HfQuantization['role'] = 'Weights',
): HfQuantization {
  return {
    label,
    role,
    file_path: `X-${label}.gguf`,
    size_bytes: sizeMib * MIB,
    size_mb: sizeMib,
    is_sharded: false,
    shard_count: null,
    projector,
    // The daemon's verdict; a projector or helper row carries none.
    fit: role === 'Weights' ? { kind: 'fits', ctx: 32768 } : null,
    fit_word: role === 'Weights' ? 'fits with 32K' : null,
  };
}

const F16 = { file_path: 'mmproj-F16.gguf', size_bytes: 880 * MIB };
const Q8 = { file_path: 'sub/X.mmproj-Q8_0.gguf', size_bytes: 629 * MIB };

async function preview(quantizations: HfQuantization[], image: HfImagePreview | null = null) {
  listing.current = quantizations;
  imageListing.current = image;
  const onDownload = vi.fn();
  const { rerender } = render(<HfModelPreview model={MODEL} onDownload={onDownload} />);
  await screen.findByText('Quant');
  return { onDownload, rerender, user: userEvent.setup() };
}

describe('HfModelPreview — the projector that comes with a download', () => {
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

  it('marks a draft and offers no download of it, and lists no projector as a row', async () => {
    await preview([
      quant('Q4_K_P', 16000, null),
      quant('FastMTP-32K', 860, null, 'Sidecar'),
      quant('mmproj-BF16', 880, null, 'Projector'),
    ]);

    const draft = screen.getByTestId('quant-row-FastMTP-32K');
    expect(within(draft).getByText('draft or helper')).toBeInTheDocument();
    expect(within(draft).getByRole('button', { name: 'Download' })).toBeDisabled();
    expect(
      within(screen.getByTestId('quant-row-Q4_K_P')).getByRole('button', { name: 'Download' }),
    ).toBeEnabled();
    expect(screen.queryByTestId('quant-row-mmproj-BF16')).not.toBeInTheDocument();
  });

  it("shows the daemon's verdict on each model in its own words, and none for a helper", async () => {
    await preview([
      { ...quant('Q8_0', 28000, Q8), fit: { kind: 'too_big', need_bytes: 2, have_bytes: 1 }, fit_word: 'too big' },
      quant('draft', 300, null, 'Sidecar'),
    ]);

    expect(within(screen.getByTestId('quant-row-Q8_0')).getByLabelText('too big')).toBeInTheDocument();
    expect(within(screen.getByTestId('quant-row-draft')).queryByLabelText(/fits|too big|tight|unknown/)).toBeNull();
  });
});

const GIB = 1024 * MIB;

/** Flux.1's preview: the VAE already here, the two text encoders to fetch. */
const FLUX: HfImagePreview = {
  family: 'flux1',
  companions: [
    { role: 'vae', repo: 'unsloth/FLUX.1-schnell', file_path: 'ae.safetensors', size_bytes: 320 * MIB, present: true },
    {
      role: 'clip_l',
      repo: 'comfyanonymous/flux_text_encoders',
      file_path: 'clip_l.safetensors',
      size_bytes: 235 * MIB,
      present: false,
    },
    {
      role: 't5xxl',
      repo: 'comfyanonymous/flux_text_encoders',
      file_path: 't5xxl_fp16.safetensors',
      size_bytes: 9 * GIB,
      present: false,
    },
  ],
  fetch_bytes: 9 * GIB + 235 * MIB,
};

describe('HfModelPreview — the companions an image model\'s download fetches', () => {
  it('names the family and each companion with its role, file, repository and size', async () => {
    await preview([quant('Q8_0', 12000, null)], FLUX);

    const note = screen.getByTestId('companion-note');
    expect(note).toHaveTextContent('An image model of the Flux.1 family.');
    const t5 = within(note).getByTestId('companion-t5xxl');
    expect(t5).toHaveTextContent('T5-XXL');
    expect(within(t5).getByText('t5xxl_fp16.safetensors')).toHaveAttribute(
      'title',
      'comfyanonymous/flux_text_encoders/t5xxl_fp16.safetensors',
    );
    expect(within(t5).getByText('comfyanonymous/flux_text_encoders')).toBeInTheDocument();
    expect(within(t5).getByText('9 GiB')).toBeInTheDocument();
    expect(within(note).getByTestId('companion-clip_l')).toHaveTextContent('235 MiB');
  });

  it('says "already here" for a companion in the models directory, and only for it', async () => {
    await preview([quant('Q8_0', 12000, null)], FLUX);

    expect(screen.getByTestId('companion-vae')).toHaveTextContent('already here');
    expect(screen.getByTestId('companion-clip_l')).not.toHaveTextContent('already here');
    expect(screen.getByTestId('companion-t5xxl')).not.toHaveTextContent('already here');
  });

  it('says what the companions add to a download, from the daemon\'s total', async () => {
    await preview([quant('Q8_0', 12000, null)], FLUX);

    expect(screen.getByTestId('companion-total')).toHaveTextContent(
      'Beside the weights, the download fetches 9.23 GiB.',
    );
  });

  it('says the weights are all a download fetches when every companion is here', async () => {
    const here = { ...FLUX, companions: FLUX.companions.map((c) => ({ ...c, present: true })), fetch_bytes: 0 };
    await preview([quant('Q8_0', 12000, null)], here);

    expect(screen.getByTestId('companion-total')).toHaveTextContent(
      'Every one is already here, so the download fetches the weights alone.',
    );
  });

  it('stays the same whichever quantization is selected', async () => {
    const { user } = await preview([quant('Q4_0', 7000, null), quant('Q8_0', 12000, null)], FLUX);

    await user.click(screen.getByText('Q8_0'));

    expect(screen.getByTestId('companion-total')).toHaveTextContent('9.23 GiB');
  });

  it('says the weights are all a family needs when its recipe names no companion', async () => {
    await preview([quant('Q8_0', 7000, null)], { family: 'sdxl', companions: [], fetch_bytes: 0 });

    const note = screen.getByTestId('companion-note');
    expect(note).toHaveTextContent('An image model of the SDXL family.');
    expect(note).toHaveTextContent('Its weights file is all it needs.');
    expect(screen.queryByTestId('companion-total')).not.toBeInTheDocument();
  });

  it('drops the last repository\'s companions as soon as another is picked', async () => {
    const { onDownload, rerender } = await preview([quant('Q8_0', 12000, null)], FLUX);
    expect(screen.getByTestId('companion-note')).toBeInTheDocument();

    rerender(<HfModelPreview model={{ ...MODEL, id: STILL_LOADING }} onDownload={onDownload} />);

    expect(screen.queryByTestId('companion-note')).not.toBeInTheDocument();
  });

  it('shows nothing for a repository that is not an image model', async () => {
    await preview([quant('Q4_K_M', 16000, F16)]);

    expect(screen.queryByTestId('companion-note')).not.toBeInTheDocument();
  });
});
