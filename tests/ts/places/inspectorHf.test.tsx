/**
 * The inspector's Hugging Face subject, at `/get/:owner/:name` through the
 * real routes: the repo read by its address (so a reload keeps it), the
 * summary with tool calling's confidence on hover, "Pick a size" with the
 * largest variant that fits selected and only it carrying Download, which
 * queues that variant; the daemon's fit in its own words; a draft dimmed
 * and not selectable; no projector as a row; the projector the selected
 * variant brings; an image model's companions with "already here"; the
 * queue-full reason; and the one line for a repo with no model.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import type { HfImagePreview, HfModelSummary, HfQuantization } from '../../../src/types';

const hub = vi.hoisted(() => ({ quants: [] as unknown[], image: null as unknown, queue: null as unknown }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => []),
  listServers: vi.fn(async () => []),
  listPairedModels: vi.fn(async () => null),
  getDownloadQueue: vi.fn(async () => hub.queue),
  getHfModelSummary: vi.fn(),
  getHfQuantizations: vi.fn(async (id: string) => ({ model_id: id, quantizations: hub.quants, image: hub.image })),
  getHfToolSupport: vi.fn(async () => ({ supports_tool_calls: true, confidence: 0.9, detected_format: 'hermes' })),
  queueDownload: vi.fn(async () => ({ id: 'q-1' })),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true })),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { cleanupDownloadEvents, initDownloadEvents } from '../../../src/services/downloadsEvents';
import { renderApp } from './harness';
import { queueSnapshot } from '../fixtures/downloads';

const MIB = 1024 * 1024;
const REPO = 'owner/X-GGUF';
const SUMMARY: HfModelSummary = {
  id: REPO, name: 'X-GGUF', author: 'owner', downloads: 1200, likes: 30, last_modified: null, parameters_b: 8, description: 'A small model.', tags: ['gguf'],
};

function quant(label: string, sizeMib: number, extra: Partial<HfQuantization> = {}): HfQuantization {
  const weights = (extra.role ?? 'Weights') === 'Weights';
  return {
    label, role: 'Weights', file_path: `X-${label}.gguf`, size_bytes: sizeMib * MIB, size_mb: sizeMib, is_sharded: false, shard_count: null, projector: null,
    fit: weights ? { kind: 'fits', ctx: 32768 } : null,
    fit_word: weights ? 'fits with 32K' : null,
    ...extra,
  };
}
const TOO_BIG = { fit: { kind: 'too_big' as const, need_bytes: 2, have_bytes: 1 }, fit_word: 'too big' };
const F16 = { file_path: 'mmproj-F16.gguf', size_bytes: 880 * MIB };

function renderHf(quants: HfQuantization[], image: HfImagePreview | null = null) {
  hub.quants = quants;
  hub.image = image;
  initModelEvents();
  initDownloadEvents();
  return renderApp(`/get/${REPO}`);
}

const inspector = () => screen.getByRole('complementary', { name: 'Inspector' });
const size = (label: string) => within(inspector()).getByRole('option', { name: new RegExp(`^${label}`) });

describe('the inspector, a Hugging Face repo', () => {
  beforeEach(() => {
    hub.queue = queueSnapshot();
    for (const mock of Object.values(transport)) mock.mockClear();
    transport.getHfModelSummary.mockImplementation(async () => SUMMARY);
  });
  afterEach(() => {
    cleanupModelEvents();
    cleanupDownloadEvents();
  });

  it('reads the repo by its address, so a reload keeps it, and sums it up with the confidence on hover', async () => {
    renderHf([quant('Q4_K_M', 4000)]);
    expect(await within(inspector()).findByRole('heading', { name: REPO })).toBeInTheDocument();
    expect(transport.getHfModelSummary).toHaveBeenCalledWith(REPO);
    expect(await within(inspector()).findByText('calls tools')).toHaveAttribute('title', '90% confident, hermes format');
    expect(within(inspector()).getByText(/^8\.0B/)).toBeInTheDocument();
  });

  it('selects the largest variant that fits, and only it carries Download, which queues it', async () => {
    renderHf([quant('Q4_K_M', 4000), quant('Q6_K', 6000), quant('Q8_0', 8000, TOO_BIG)]);
    await within(inspector()).findByRole('listbox', { name: 'Sizes' });

    expect(size('Q6_K')).toHaveAttribute('aria-selected', 'true');
    expect(within(inspector()).getAllByRole('button', { name: 'Download' })).toHaveLength(1);
    expect(within(size('Q8_0')).getByText('too big')).toBeInTheDocument();
    expect(within(size('Q6_K')).getByText('5.86 GiB')).toHaveAttribute('title', 'X-Q6_K.gguf');

    const user = userEvent.setup();
    await user.click(within(size('Q6_K')).getByRole('button', { name: 'Download' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: REPO, quantization: 'Q6_K' }));

    await user.click(size('Q4_K_M'));
    expect(size('Q4_K_M')).toHaveAttribute('aria-selected', 'true');
    expect(within(size('Q6_K')).queryByRole('button', { name: 'Download' })).not.toBeInTheDocument();
  });

  it('selects the smallest when none fits, and names a sharded variant\'s files on hover', async () => {
    renderHf([quant('Q4_K_M', 4000, TOO_BIG), quant('Q8_0', 8000, { ...TOO_BIG, is_sharded: true, shard_count: 3 })]);
    await within(inspector()).findByRole('listbox', { name: 'Sizes' });
    expect(size('Q4_K_M')).toHaveAttribute('aria-selected', 'true');
    expect(within(size('Q8_0')).getByText('7.81 GiB')).toHaveAttribute('title', '3 files, from X-Q8_0.gguf');
  });

  it('dims a draft as not a model on its own, never selects it, and lists no projector as a row', async () => {
    renderHf([quant('Q4_K_P', 16000), quant('FastMTP', 20000, { role: 'Sidecar' }), quant('mmproj-BF16', 880, { role: 'Projector' })]);
    await within(inspector()).findByRole('listbox', { name: 'Sizes' });

    const draft = size('FastMTP');
    expect(draft).toHaveTextContent('not a model on its own');
    expect(draft).toHaveAttribute('aria-disabled', 'true');
    await userEvent.setup().click(draft);
    expect(draft).toHaveAttribute('aria-selected', 'false');
    expect(within(draft).queryByRole('button', { name: 'Download' })).not.toBeInTheDocument();
    expect(within(inspector()).queryByRole('option', { name: /mmproj/ })).not.toBeInTheDocument();
  });

  it("says the projector the selected variant brings, and an image model's companions", async () => {
    const flux: HfImagePreview = {
      family: 'flux1',
      companions: [{ role: 'vae', repo: 'unsloth/FLUX.1-schnell', file_path: 'ae.safetensors', size_bytes: 320 * MIB, present: true }],
      fetch_bytes: 0,
    };
    renderHf([quant('Q4_K_M', 4000, { projector: F16 })], flux);
    const note = await within(inspector()).findByTestId('projector-note');
    expect(note).toHaveTextContent('Brings mmproj-F16.gguf (880 MiB) for images');
    expect(note.getAttribute('title')).toMatch(/context shift and cache reuse by shifting are off/);
    expect(within(inspector()).getByTestId('companion-vae')).toHaveTextContent('already here');
    expect(within(inspector()).getByTestId('companion-total')).toHaveTextContent('Every one is already here');
  });

  it('says why Download waits while the queue is full', async () => {
    hub.queue = queueSnapshot({ full: true });
    renderHf([quant('Q4_K_M', 4000)]);
    await waitFor(() => expect(within(inspector()).getByRole('button', { name: 'Download' })).toBeDisabled());
    expect(within(inspector()).getByText('The download queue is full; Download waits for room.')).toBeInTheDocument();
  });

  it('says so for a repo with no model in it', async () => {
    renderHf([quant('mmproj-BF16', 880, { role: 'Projector' })]);
    expect(await within(inspector()).findByText('This repo has no GGUF files.')).toBeInTheDocument();
  });
});
