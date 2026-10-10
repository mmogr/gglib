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
const empty = async (_request: HfSearchRequest): Promise<HfSearchResponse> => ({ models: [], has_more: false, page: 0, total_count: null });
const browseHfModels = vi.fn(empty);

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
    expect(browseHfModels.mock.calls.at(-1)?.[0].page).toBe(0);
    expect(screen.getByRole('button', { name: 'Image' })).toHaveAttribute('aria-pressed', 'true');

    browseHfModels.mockClear();
    await user.click(screen.getByRole('button', { name: 'Chat' }));
    await waitFor(() => expect(kinds()).toContain('chat'));
    expect(new Set(kinds())).toEqual(new Set(['chat']));
  });

  it('asks for the next page with the kind pressed', async () => {
    browseHfModels.mockImplementation(async (request) => ({
      models: [
        {
          id: `owner/m-${request.page}`,
          name: `m-${request.page}`,
          author: 'owner',
          downloads: 0,
          likes: 0,
          last_modified: null,
          parameters_b: null,
          description: null,
          tags: [],
        },
      ],
      has_more: true,
      page: request.page,
      total_count: null,
    }));
    render(<HuggingFaceBrowser />);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Image' }));
    await waitFor(() => expect(kinds()).toContain('image'));
    await screen.findByRole('button', { name: 'Load More' });

    browseHfModels.mockClear();
    await user.click(screen.getByRole('button', { name: 'Load More' }));

    await waitFor(() => expect(browseHfModels).toHaveBeenCalledTimes(1));
    expect(browseHfModels.mock.calls[0][0]).toMatchObject({ kind: 'image', page: 1 });
  });
});
