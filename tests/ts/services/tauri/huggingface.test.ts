/**
 * Tests for HuggingFace service functions.
 * 
 * Tests HuggingFace model browsing and quantization operations in both Tauri and Web modes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock platform detection - must be before importing service functions
let mockIsTauriApp = true;
vi.mock('../../../../src/utils/platform', () => ({
  get isTauriApp() {
    return mockIsTauriApp;
  },
}));

// Mock apiBase
vi.mock('../../../../src/utils/apiBase', () => ({
  getApiBase: vi.fn(() => Promise.resolve('http://localhost:9887/api')),
}));

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

// Import after mocks are set up
import {
  browseHfModels,
  getHfQuantizations,
  getHfToolSupport,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('HuggingFace Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('browseHfModels', () => {
    it('invokes browse_hf_models in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = {
        models: [{ id: 'TheBloke/Llama-2-7B-GGUF', downloads: 1000 }],
        total: 1,
      };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await browseHfModels({ query: 'llama' });

      expect(mockInvoke).toHaveBeenCalledWith('browse_hf_models', { request: { query: 'llama' } });
      expect(result).toEqual(response);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const response = { models: [], total: 0 };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: response }),
      });

      const result = await browseHfModels({ query: 'mistral', limit: 10 });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/hf/browse'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ query: 'mistral', limit: 10 }),
        })
      );
      expect(result).toEqual(response);
    });

    it('handles pagination parameters', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ models: [], total: 100 });

      await browseHfModels({ query: 'test', page: 2, limit: 20 });

      expect(mockInvoke).toHaveBeenCalledWith('browse_hf_models', {
        request: { query: 'test', page: 2, limit: 20 },
      });
    });
  });

  describe('getHfQuantizations', () => {
    it('invokes get_hf_quantizations in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = {
        quantizations: [
          { name: 'Q4_K_M', size_bytes: 4000000000 },
          { name: 'Q5_K_S', size_bytes: 5000000000 },
        ],
      };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await getHfQuantizations('TheBloke/Llama-2-7B-GGUF');

      expect(mockInvoke).toHaveBeenCalledWith('get_hf_quantizations', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
      });
      expect(result).toEqual(response);
    });

    it('encodes model ID in Web mode URL', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { quantizations: [] } }),
      });

      await getHfQuantizations('TheBloke/Llama-2-7B-GGUF');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('TheBloke%2FLlama-2-7B-GGUF'),
        undefined
      );
    });

    it('handles models with special characters', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { quantizations: [] } }),
      });

      await getHfQuantizations('org/model-name_v1.0');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining(encodeURIComponent('org/model-name_v1.0')),
        undefined
      );
    });
  });

  describe('getHfToolSupport', () => {
    it('invokes get_hf_tool_support in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = { supports_tools: true, chat_template: 'jinja' };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await getHfToolSupport('TheBloke/Llama-2-7B-GGUF');

      expect(mockInvoke).toHaveBeenCalledWith('get_hf_tool_support', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
      });
      expect(result).toEqual(response);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const response = { supports_tools: false };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: response }),
      });

      const result = await getHfToolSupport('org/model');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/hf/tool-support/'),
        undefined
      );
      expect(result).toEqual(response);
    });

    it('returns supports_tools as false when model has no tool support', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ supports_tools: false });

      const result = await getHfToolSupport('simple-model');

      expect(result.supports_tools).toBe(false);
    });
  });
});
