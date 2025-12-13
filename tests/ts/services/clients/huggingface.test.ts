/**
 * Tests for huggingface client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  browseHfModels,
  getHfQuantizations,
  getHfToolSupport,
} from '../../../../src/services/clients/huggingface';
import { getTransport, _resetTransport } from '../../../../src/services/transport';
import type { HfSearchResponse, HfQuantizationsResponse, HfToolSupportResponse } from '../../../../src/types';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    browseHfModels: vi.fn(),
    getHfQuantizations: vi.fn(),
    getHfToolSupport: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/huggingface', () => {
  const mockTransport = getTransport();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('browseHfModels', () => {
    it('delegates to transport.browseHfModels()', async () => {
      const params = { query: 'llama', page: 1, limit: 10 };
      const mockResponse: HfSearchResponse = {
        models: [],
        total: 0,
        page: 1,
        has_more: false,
      };
      vi.mocked(mockTransport.browseHfModels).mockResolvedValue(mockResponse);

      const result = await browseHfModels(params);

      expect(mockTransport.browseHfModels).toHaveBeenCalledWith(params);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('getHfQuantizations', () => {
    it('delegates to transport.getHfQuantizations()', async () => {
      const modelId = 'TheBloke/Llama-2-7B-GGUF';
      const mockResponse: HfQuantizationsResponse = {
        quantizations: [
          { name: 'Q4_K_M', size_bytes: 4_000_000_000, bpw: 4.5 },
        ],
      };
      vi.mocked(mockTransport.getHfQuantizations).mockResolvedValue(mockResponse);

      const result = await getHfQuantizations(modelId);

      expect(mockTransport.getHfQuantizations).toHaveBeenCalledWith(modelId);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('getHfToolSupport', () => {
    it('delegates to transport.getHfToolSupport()', async () => {
      const modelId = 'TheBloke/Llama-2-7B-GGUF';
      const mockResponse: HfToolSupportResponse = {
        supports_tools: true,
        tool_format: 'chatml',
      };
      vi.mocked(mockTransport.getHfToolSupport).mockResolvedValue(mockResponse);

      const result = await getHfToolSupport(modelId);

      expect(mockTransport.getHfToolSupport).toHaveBeenCalledWith(modelId);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      vi.mocked(mockTransport.browseHfModels).mockResolvedValue({
        models: [],
        total: 0,
        page: 1,
        has_more: false,
      });
      vi.mocked(mockTransport.getHfQuantizations).mockResolvedValue({ quantizations: [] });
      vi.mocked(mockTransport.getHfToolSupport).mockResolvedValue({
        supports_tools: false,
        tool_format: null,
      });

      await browseHfModels({ query: 'test' });
      await getHfQuantizations('test/model');
      await getHfToolSupport('test/model');

      expect(mockTransport.browseHfModels).toHaveBeenCalled();
      expect(mockTransport.getHfQuantizations).toHaveBeenCalled();
      expect(mockTransport.getHfToolSupport).toHaveBeenCalled();
    });
  });
});
