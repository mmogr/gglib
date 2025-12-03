/**
 * Tests for download service functions.
 * 
 * Tests model download operations and queue management in both Tauri and Web modes.
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
  downloadModel,
  cancelDownload,
  queueDownload,
  getDownloadQueue,
  removeFromDownloadQueue,
  reorderDownloadQueue,
  cancelShardGroup,
  clearFailedDownloads,
  searchModels,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('Download Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('downloadModel', () => {
    it('invokes download_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Download started');

      const result = await downloadModel({
        repo_id: 'TheBloke/Llama-2-7B-GGUF',
        quantization: 'Q4_K_M',
      });

      expect(mockInvoke).toHaveBeenCalledWith('download_model', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
        quantization: 'Q4_K_M',
      });
      expect(result).toBe('Download started');
    });

    it('handles Web mode errors gracefully', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        json: () => Promise.resolve({ error: 'Model not found' }),
      });

      await expect(
        downloadModel({ repo_id: 'unknown/model', quantization: 'Q4' })
      ).rejects.toThrow('Model not found');
    });
  });

  describe('cancelDownload', () => {
    it('invokes cancel_download in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Cancelled');

      const result = await cancelDownload('model-id');

      expect(mockInvoke).toHaveBeenCalledWith('cancel_download', { modelId: 'model-id' });
      expect(result).toBe('Cancelled');
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Cancelled' }),
      });

      await cancelDownload('model-id');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/download/cancel'),
        expect.objectContaining({ method: 'POST' })
      );
    });
  });

  describe('queueDownload', () => {
    it('invokes queue_download in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ position: 1, shard_count: 1 });

      const result = await queueDownload('model-id', 'Q4_K_M');

      expect(mockInvoke).toHaveBeenCalledWith('queue_download', {
        modelId: 'model-id',
        quantization: 'Q4_K_M',
      });
      expect(result).toEqual({ position: 1, shard_count: 1 });
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { position: 2, shard_count: 3 } }),
      });

      const result = await queueDownload('org/model', 'Q5_K_S');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/download/queue'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ model_id: 'org/model', quantization: 'Q5_K_S' }),
        })
      );
      expect(result).toEqual({ position: 2, shard_count: 3 });
    });
  });

  describe('getDownloadQueue', () => {
    it('invokes get_download_queue in Tauri mode', async () => {
      mockIsTauriApp = true;
      const queue = { pending: [], failed: [], max_size: 10, current: null };
      mockInvoke.mockResolvedValueOnce(queue);

      const result = await getDownloadQueue();

      expect(mockInvoke).toHaveBeenCalledWith('get_download_queue', undefined);
      expect(result).toEqual(queue);
    });

    it('returns default queue in Web mode when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await getDownloadQueue();
      expect(result).toEqual({ pending: [], failed: [], max_size: 10 });
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const queue = { pending: [{ model_id: 'test' }], failed: [], max_size: 10, current: null };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: queue }),
      });

      const result = await getDownloadQueue();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/download/queue'),
        undefined
      );
      expect(result).toEqual(queue);
    });
  });

  describe('removeFromDownloadQueue', () => {
    it('invokes remove_from_download_queue in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Removed');

      const result = await removeFromDownloadQueue('model-id');

      expect(mockInvoke).toHaveBeenCalledWith('remove_from_download_queue', { modelId: 'model-id' });
      expect(result).toBe('Removed');
    });
  });

  describe('reorderDownloadQueue', () => {
    it('invokes reorder_download_queue in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Reordered');

      const result = await reorderDownloadQueue('model-id', 0);

      expect(mockInvoke).toHaveBeenCalledWith('reorder_download_queue', {
        modelId: 'model-id',
        newPosition: 0,
      });
      expect(result).toBe('Reordered');
    });
  });

  describe('cancelShardGroup', () => {
    it('invokes cancel_shard_group in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Cancelled');

      const result = await cancelShardGroup('model-id');

      expect(mockInvoke).toHaveBeenCalledWith('cancel_shard_group', { groupId: 'model-id' });
      expect(result).toBe('Cancelled');
    });
  });

  describe('clearFailedDownloads', () => {
    it('invokes clear_failed_downloads in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Cleared');

      const result = await clearFailedDownloads();

      expect(mockInvoke).toHaveBeenCalledWith('clear_failed_downloads', undefined);
      expect(result).toBe('Cleared');
    });
  });

  describe('searchModels', () => {
    it('invokes search_models in Tauri mode', async () => {
      mockIsTauriApp = true;
      const searchResults = [{ id: 1, name: 'llama' }];
      mockInvoke.mockResolvedValueOnce(searchResults);

      const result = await searchModels('llama');

      expect(mockInvoke).toHaveBeenCalledWith('search_models', {
        query: 'llama',
        sort: 'downloads',
        limit: 20,
        ggufOnly: true,
      });
      expect(result).toEqual(searchResults);
    });
  });
});
