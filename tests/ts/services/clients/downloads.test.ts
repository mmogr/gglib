/**
 * Tests for downloads client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  getDownloadQueue,
  queueDownload,
  cancelDownload,
  removeFromQueue,
  clearFailedDownloads,
  cancelShardGroup,
  reorderQueue,
} from '../../../../src/services/clients/downloads';
import { getTransport, _resetTransport } from '../../../../src/services/transport';
import type { DownloadQueueStatus } from '../../../../src/services/transport/types/downloads';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    getDownloadQueue: vi.fn(),
    queueDownload: vi.fn(),
    cancelDownload: vi.fn(),
    removeFromQueue: vi.fn(),
    clearFailedDownloads: vi.fn(),
    cancelShardGroup: vi.fn(),
    reorderQueue: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/downloads', () => {
  const mockTransport = getTransport();

  const mockQueueStatus: DownloadQueueStatus = {
    current: null,
    pending: [],
    failed: [],
    max_size: 10,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('getDownloadQueue', () => {
    it('delegates to transport.getDownloadQueue()', async () => {
      vi.mocked(mockTransport.getDownloadQueue).mockResolvedValue(mockQueueStatus);

      const result = await getDownloadQueue();

      expect(mockTransport.getDownloadQueue).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockQueueStatus);
    });
  });

  describe('queueDownload', () => {
    it('delegates to transport.queueDownload()', async () => {
      const params = { modelId: 'TheBloke/Llama-2-7B-GGUF', quantization: 'Q4_K_M' };
      const downloadId = 'download-123';
      vi.mocked(mockTransport.queueDownload).mockResolvedValue(downloadId);

      const result = await queueDownload(params);

      expect(mockTransport.queueDownload).toHaveBeenCalledWith(params);
      expect(result).toBe(downloadId);
    });
  });

  describe('cancelDownload', () => {
    it('delegates to transport.cancelDownload()', async () => {
      const downloadId = 'download-123';
      vi.mocked(mockTransport.cancelDownload).mockResolvedValue(undefined);

      await cancelDownload(downloadId);

      expect(mockTransport.cancelDownload).toHaveBeenCalledWith(downloadId);
    });
  });

  describe('removeFromQueue', () => {
    it('delegates to transport.removeFromQueue()', async () => {
      const downloadId = 'download-123';
      vi.mocked(mockTransport.removeFromQueue).mockResolvedValue(undefined);

      await removeFromQueue(downloadId);

      expect(mockTransport.removeFromQueue).toHaveBeenCalledWith(downloadId);
    });
  });

  describe('clearFailedDownloads', () => {
    it('delegates to transport.clearFailedDownloads()', async () => {
      vi.mocked(mockTransport.clearFailedDownloads).mockResolvedValue(undefined);

      await clearFailedDownloads();

      expect(mockTransport.clearFailedDownloads).toHaveBeenCalledTimes(1);
    });
  });

  describe('cancelShardGroup', () => {
    it('delegates to transport.cancelShardGroup()', async () => {
      const groupId = 'group-abc';
      vi.mocked(mockTransport.cancelShardGroup).mockResolvedValue(undefined);

      await cancelShardGroup(groupId);

      expect(mockTransport.cancelShardGroup).toHaveBeenCalledWith(groupId);
    });
  });

  describe('reorderQueue', () => {
    it('delegates to transport.reorderQueue()', async () => {
      const ids = ['download-1', 'download-2', 'download-3'];
      vi.mocked(mockTransport.reorderQueue).mockResolvedValue(undefined);

      await reorderQueue(ids);

      expect(mockTransport.reorderQueue).toHaveBeenCalledWith(ids);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      vi.mocked(mockTransport.getDownloadQueue).mockResolvedValue(mockQueueStatus);
      vi.mocked(mockTransport.queueDownload).mockResolvedValue('id');
      vi.mocked(mockTransport.cancelDownload).mockResolvedValue(undefined);
      vi.mocked(mockTransport.removeFromQueue).mockResolvedValue(undefined);
      vi.mocked(mockTransport.clearFailedDownloads).mockResolvedValue(undefined);
      vi.mocked(mockTransport.cancelShardGroup).mockResolvedValue(undefined);
      vi.mocked(mockTransport.reorderQueue).mockResolvedValue(undefined);

      await getDownloadQueue();
      await queueDownload({ modelId: 'test', quantization: 'Q4' });
      await cancelDownload('id');
      await removeFromQueue('id');
      await clearFailedDownloads();
      await cancelShardGroup('group');
      await reorderQueue(['a', 'b']);

      expect(mockTransport.getDownloadQueue).toHaveBeenCalled();
      expect(mockTransport.queueDownload).toHaveBeenCalled();
      expect(mockTransport.cancelDownload).toHaveBeenCalled();
      expect(mockTransport.removeFromQueue).toHaveBeenCalled();
      expect(mockTransport.clearFailedDownloads).toHaveBeenCalled();
      expect(mockTransport.cancelShardGroup).toHaveBeenCalled();
      expect(mockTransport.reorderQueue).toHaveBeenCalled();
    });
  });
});
