/**
 * Tests for model service functions.
 * 
 * Tests CRUD operations for local GGUF models in both Tauri and Web modes.
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
  listModels,
  getModel,
  addModel,
  removeModel,
  updateModel,
  getModelFilterOptions,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('Model Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('listModels', () => {
    const mockModels = [
      { id: 1, name: 'llama-7b', file_path: '/models/llama.gguf' },
      { id: 2, name: 'mistral-7b', file_path: '/models/mistral.gguf' },
    ];

    it('uses invoke in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(mockModels);

      const result = await listModels();

      expect(mockInvoke).toHaveBeenCalledWith('list_models', undefined);
      expect(result).toEqual(mockModels);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: mockModels }),
      });

      const result = await listModels();

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/models', undefined);
      expect(result).toEqual(mockModels);
    });

    it('throws error on fetch failure in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        statusText: 'Internal Server Error',
      });

      await expect(listModels()).rejects.toThrow('Failed to fetch models');
    });

    it('returns empty array when data is undefined in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await listModels();
      expect(result).toEqual([]);
    });
  });

  describe('getModel', () => {
    it('finds model by id in Tauri mode', async () => {
      mockIsTauriApp = true;
      const models = [
        { id: 1, name: 'llama-7b' },
        { id: 2, name: 'mistral-7b' },
      ];
      mockInvoke.mockResolvedValueOnce(models);

      const result = await getModel(2);

      expect(result).toEqual({ id: 2, name: 'mistral-7b' });
    });

    it('throws when model not found in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce([]);

      await expect(getModel(999)).rejects.toThrow('Model 999 not found');
    });

    it('uses REST API in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { id: 1, name: 'llama' } }),
      });

      const result = await getModel(1);

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/models/1', undefined);
      expect(result).toEqual({ id: 1, name: 'llama' });
    });
  });

  describe('addModel', () => {
    it('invokes add_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model added successfully');

      const result = await addModel('/path/to/model.gguf');

      expect(mockInvoke).toHaveBeenCalledWith('add_model', { filePath: '/path/to/model.gguf' });
      expect(result).toBe('Model added successfully');
    });

    it('posts to API in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { name: 'new-model' } }),
      });

      const result = await addModel('/path/to/model.gguf');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ file_path: '/path/to/model.gguf' }),
        })
      );
      expect(result).toBe('Model added: new-model');
    });
  });

  describe('removeModel', () => {
    it('invokes remove_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model removed');

      await removeModel('1', true);

      expect(mockInvoke).toHaveBeenCalledWith('remove_model', { identifier: '1', force: true });
    });

    it('defaults force to false', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model removed');

      await removeModel('1');

      expect(mockInvoke).toHaveBeenCalledWith('remove_model', { identifier: '1', force: false });
    });
  });

  describe('updateModel', () => {
    it('invokes update_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      const updatedModel = { id: 1, name: 'updated-name' };
      mockInvoke.mockResolvedValueOnce(updatedModel);

      const result = await updateModel(1, { name: 'updated-name' });

      expect(mockInvoke).toHaveBeenCalledWith('update_model', {
        id: 1,
        updates: { name: 'updated-name' },
      });
      expect(result).toEqual(updatedModel);
    });

    it('uses PUT in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { id: 1, name: 'updated' } }),
      });

      await updateModel(1, { name: 'updated' });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1',
        expect.objectContaining({ method: 'PUT' })
      );
    });
  });

  describe('getModelFilterOptions', () => {
    it('invokes get_model_filter_options in Tauri mode', async () => {
      mockIsTauriApp = true;
      const options = {
        architectures: ['llama', 'mistral'],
        quantizations: ['Q4_K_M', 'Q5_K_S'],
        tags: ['chat', 'code'],
      };
      mockInvoke.mockResolvedValueOnce(options);

      const result = await getModelFilterOptions();

      expect(mockInvoke).toHaveBeenCalledWith('get_model_filter_options', undefined);
      expect(result).toEqual(options);
    });
  });
});
