/**
 * Tests for tags service functions.
 * 
 * Tests model tagging operations in both Tauri and Web modes.
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
  listTags,
  addModelTag,
  removeModelTag,
  getModelTags,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('Tags Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('listTags', () => {
    it('invokes list_tags in Tauri mode', async () => {
      mockIsTauriApp = true;
      const tags = ['chat', 'code', 'reasoning', 'vision'];
      mockInvoke.mockResolvedValueOnce(tags);

      const result = await listTags();

      expect(mockInvoke).toHaveBeenCalledWith('list_tags', undefined);
      expect(result).toEqual(tags);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const tags = ['chat', 'code'];
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: tags }),
      });

      const result = await listTags();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/tags'),
        undefined
      );
      expect(result).toEqual(tags);
    });

    it('returns empty array when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await listTags();
      expect(result).toEqual([]);
    });
  });

  describe('addModelTag', () => {
    it('invokes add_model_tag in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Tag added');

      const result = await addModelTag(1, 'chat');

      expect(mockInvoke).toHaveBeenCalledWith('add_model_tag', { modelId: 1, tag: 'chat' });
      expect(result).toBe('Tag added');
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Added' }),
      });

      await addModelTag(42, 'reasoning');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/models/42/tags'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ tag: 'reasoning' }),
        })
      );
    });
  });

  describe('removeModelTag', () => {
    it('invokes remove_model_tag in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Tag removed');

      const result = await removeModelTag(1, 'chat');

      expect(mockInvoke).toHaveBeenCalledWith('remove_model_tag', { modelId: 1, tag: 'chat' });
      expect(result).toBe('Tag removed');
    });

    it('uses DELETE in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Removed' }),
      });

      await removeModelTag(42, 'vision');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/models/42/tags'),
        expect.objectContaining({
          method: 'DELETE',
          body: JSON.stringify({ tag: 'vision' }),
        })
      );
    });
  });

  describe('getModelTags', () => {
    it('invokes get_model_tags in Tauri mode', async () => {
      mockIsTauriApp = true;
      const modelTags = ['chat', 'code'];
      mockInvoke.mockResolvedValueOnce(modelTags);

      const result = await getModelTags(1);

      expect(mockInvoke).toHaveBeenCalledWith('get_model_tags', { modelId: 1 });
      expect(result).toEqual(modelTags);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const modelTags = ['reasoning', 'agent'];
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: modelTags }),
      });

      const result = await getModelTags(42);

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/models/42/tags'),
        undefined
      );
      expect(result).toEqual(modelTags);
    });

    it('returns empty array when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await getModelTags(1);
      expect(result).toEqual([]);
    });
  });
});
