/**
 * Tests for the hardware-sized model suggestion.
 *
 * The case that matters most is the null one: "nothing fits this machine" is
 * a real answer, and rendering a card with blanks in it would be worse than
 * saying so.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { RecommendedModel } from '../../../src/components/ModelLibraryPanel/RecommendedModel';
import { getRecommendedModel } from '../../../src/services/transport/api/setup';
import type { ModelRecommendation } from '../../../src/types/setup';

vi.mock('../../../src/services/transport/api/setup', () => ({
  getRecommendedModel: vi.fn(),
}));

const mockGet = vi.mocked(getRecommendedModel);

const rec: ModelRecommendation = {
  repo: 'unsloth/Qwen3-30B-A3B-GGUF',
  quantization: 'Q4_K_M',
  rationale: 'mixture-of-experts: 30B of knowledge, ~3B active per token',
  // Binary, matching formatMemorySize: 21 GiB and 32 GiB exactly.
  requiredBytes: 21 * 1024 ** 3,
  budgetBytes: 32 * 1024 ** 3,
  budgetSource: 'unifiedMemory',
  headroomBytes: 11_000_000_000,
  context: 32_768,
};

describe('RecommendedModel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('names the model with what it needs and what the machine has', async () => {
    mockGet.mockResolvedValue(rec);

    render(<RecommendedModel onUseRepo={vi.fn()} />);

    expect(await screen.findByText('unsloth/Qwen3-30B-A3B-GGUF')).toBeInTheDocument();
    // Binary units, the same convention as the fit indicators and `gglib up`.
    expect(screen.getByText('21.0 GiB')).toBeInTheDocument();
    expect(screen.getByText('32.0 GiB')).toBeInTheDocument();
    expect(screen.getByText(/unified memory/)).toBeInTheDocument();
  });

  it('says so when nothing in the shortlist fits', async () => {
    mockGet.mockResolvedValue(null);

    render(<RecommendedModel onUseRepo={vi.fn()} />);

    expect(await screen.findByText(/No model in gglib's shortlist fits/)).toBeInTheDocument();
  });

  it('seeds the search box rather than starting a download', async () => {
    mockGet.mockResolvedValue(rec);
    const onUseRepo = vi.fn();

    render(<RecommendedModel onUseRepo={onUseRepo} />);
    await userEvent.click(await screen.findByRole('button', { name: /Find it/ }));

    expect(onUseRepo).toHaveBeenCalledWith('unsloth/Qwen3-30B-A3B-GGUF');
  });

  it('stays out of the way when the lookup fails', async () => {
    mockGet.mockRejectedValue(new Error('offline'));

    const { container } = render(<RecommendedModel onUseRepo={vi.fn()} />);

    // Wait for the rejection to settle first — asserting immediately would
    // pass against the broken behaviour, which only renders after `loaded`.
    await vi.waitFor(() => expect(mockGet).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 0));

    // A transport failure must never be reported as "nothing fits": that is a
    // claim about the user's hardware drawn from a network error.
    expect(screen.queryByText(/No model in gglib's shortlist fits/)).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });
});
