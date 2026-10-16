/**
 * The HuggingFace browser's Chat/Image toggle: a search asks for models that
 * chat until Image is pressed, then searches again for models that draw,
 * and every page after it asks the same.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import type { HfSearchRequest, HfSearchResponse } from '../../../src/types';

/** A Hub with nothing to list. */
const empty = async (_request: HfSearchRequest): Promise<HfSearchResponse> => ({ models: [], next_cursor: null, total_count: null });
const browseHfModels = vi.fn(empty);

/** A Hub that answers each page with one repo named for the cursor it was
 * asked from, and names the cursor after it. */
async function paged(request: HfSearchRequest): Promise<HfSearchResponse> {
  const at = request.cursor ?? 'first';
  return {
    models: [
      {
        id: `owner/m-${at}`,
        name: `m-${at}`,
        author: 'owner',
        downloads: 0,
        likes: 0,
        last_modified: null,
        parameters_b: null,
        description: null,
        tags: [],
      },
    ],
    next_cursor: `after-${at}`,
    total_count: null,
  };
}

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({
    browseHfModels,
    getHfToolSupport: async () => ({ supports_tool_calls: false, confidence: 0, detected_format: null }),
  }),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getRecommendedModel: () => new Promise(() => {}),
}));
vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../src/contexts/ToastContext', () => ({
  useToastContext: () => ({ showToast: vi.fn() }),
}));

import HuggingFaceBrowser from '../../../src/components/HuggingFaceBrowser/HuggingFaceBrowser';

/** The kind each search so far asked for, in order. */
const kinds = () => browseHfModels.mock.calls.map(([request]) => request.kind);

describe('HuggingFaceBrowser — the Chat/Image toggle', () => {
  beforeEach(() => {
    browseHfModels.mockReset();
    browseHfModels.mockImplementation(empty);
  });

  it('asks for models that chat at first, with Chat pressed', async () => {
    render(<HuggingFaceBrowser />);

    await waitFor(() => expect(browseHfModels).toHaveBeenCalled());
    expect(new Set(kinds())).toEqual(new Set(['chat']));
    expect(screen.getByRole('button', { name: 'Chat' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'Image' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('searches again for models that draw when Image is pressed, and for models that chat when Chat is', async () => {
    render(<HuggingFaceBrowser />);
    await waitFor(() => expect(browseHfModels).toHaveBeenCalled());
    const user = userEvent.setup();

    browseHfModels.mockClear();
    await user.click(screen.getByRole('button', { name: 'Image' }));
    await waitFor(() => expect(kinds()).toContain('image'));
    expect(new Set(kinds())).toEqual(new Set(['image']));
    expect(browseHfModels.mock.calls.at(-1)?.[0].cursor).toBeNull();
    expect(screen.getByRole('button', { name: 'Image' })).toHaveAttribute('aria-pressed', 'true');

    browseHfModels.mockClear();
    await user.click(screen.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(kinds()).toContain('chat'));
    expect(new Set(kinds())).toEqual(new Set(['chat']));
  });

  it('asks for the next page with the kind pressed', async () => {
    browseHfModels.mockImplementation(paged);
    render(<HuggingFaceBrowser />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Image' }));
    await waitFor(() => expect(kinds()).toContain('image'));
    await screen.findByRole('button', { name: 'Load More' });

    browseHfModels.mockClear();
    await user.click(screen.getByRole('button', { name: 'Load More' }));

    await waitFor(() => expect(browseHfModels).toHaveBeenCalledTimes(1));
    expect(browseHfModels.mock.calls[0][0]).toMatchObject({ kind: 'image', cursor: 'after-first' });
  });
});

describe('HuggingFaceBrowser — load more', () => {
  beforeEach(() => {
    browseHfModels.mockReset();
    browseHfModels.mockImplementation(paged);
  });

  it("carries on from each page's cursor and adds its repos under the shown ones", async () => {
    render(<HuggingFaceBrowser />);
    const user = userEvent.setup();
    await screen.findByText('m-first');

    await user.click(screen.getByRole('button', { name: 'Load More' }));
    await screen.findByText('m-after-first');
    await user.click(screen.getByRole('button', { name: 'Load More' }));
    await screen.findByText('m-after-after-first');

    const loads = browseHfModels.mock.calls.map(([request]) => request.cursor).filter((c) => c !== null);
    expect(loads).toEqual(['after-first', 'after-after-first']);
    // A page is as many repos as the Hub gives at once, so a filter that drops
    // most of them still leaves something to show.
    expect(new Set(browseHfModels.mock.calls.map(([request]) => request.limit))).toEqual(new Set([100]));
    expect(screen.getByText('m-first')).toBeInTheDocument();
    expect(screen.getByText('m-after-first')).toBeInTheDocument();
  });

  it('searches again from the first page when the words change, so a cursor never outlives its search', async () => {
    render(<HuggingFaceBrowser />);
    const user = userEvent.setup();
    await screen.findByText('m-first');

    browseHfModels.mockClear();
    await user.type(screen.getByPlaceholderText(/Search, paste/), 'q');

    await waitFor(() => expect(browseHfModels).toHaveBeenCalled());
    expect(browseHfModels.mock.calls.at(-1)![0]).toMatchObject({ query: 'q', cursor: null });
  });

  it('offers no more once the Hub names no next page', async () => {
    browseHfModels.mockImplementation(async (request) => ({ ...(await paged(request)), next_cursor: null }));
    render(<HuggingFaceBrowser />);

    await screen.findByText('m-first');
    expect(screen.queryByRole('button', { name: 'Load More' })).not.toBeInTheDocument();
  });
});
