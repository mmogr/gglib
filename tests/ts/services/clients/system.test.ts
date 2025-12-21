/**
 * Tests for system client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  getSystemMemory,
  getModelsDirectory,
  setModelsDirectory,
} from '../../../../src/services/clients/system';
import { getTransport, _resetTransport } from '../../../../src/services/transport';
import type { SystemMemoryInfo, ModelsDirectoryInfo } from '../../../../src/types';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    getSystemMemory: vi.fn(),
    getModelsDirectory: vi.fn(),
    setModelsDirectory: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/system', () => {
  const mockTransport = getTransport();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('getSystemMemory', () => {
    it('delegates to transport.getSystemMemory()', async () => {
      const mockMemory: SystemMemoryInfo = {
        total_ram_bytes: 32 * 1024 * 1024 * 1024,
        available_ram_bytes: 16 * 1024 * 1024 * 1024,
        gpu_memory_bytes: 8 * 1024 * 1024 * 1024,
      };
      vi.mocked(mockTransport.getSystemMemory).mockResolvedValue(mockMemory);

      const result = await getSystemMemory();

      expect(mockTransport.getSystemMemory).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockMemory);
    });
  });

  describe('getModelsDirectory', () => {
    it('delegates to transport.getModelsDirectory()', async () => {
      const mockDirInfo: ModelsDirectoryInfo = {
        path: '/Users/test/.gglib/models',
        exists: true,
        model_count: 5,
        total_size_bytes: 10 * 1024 * 1024 * 1024,
      };
      vi.mocked(mockTransport.getModelsDirectory).mockResolvedValue(mockDirInfo);

      const result = await getModelsDirectory();

      expect(mockTransport.getModelsDirectory).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockDirInfo);
    });
  });

  describe('setModelsDirectory', () => {
    it('delegates to transport.setModelsDirectory()', async () => {
      const newPath = '/Users/test/new-models';
      vi.mocked(mockTransport.setModelsDirectory).mockResolvedValue(undefined);

      await setModelsDirectory(newPath);

      expect(mockTransport.setModelsDirectory).toHaveBeenCalledWith(newPath);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      vi.mocked(mockTransport.getSystemMemory).mockResolvedValue({
        total_ram_bytes: 32 * 1024 * 1024 * 1024,
        available_ram_bytes: 16 * 1024 * 1024 * 1024,
        gpu_memory_bytes: null,
      });
      vi.mocked(mockTransport.getModelsDirectory).mockResolvedValue({
        path: '/test',
        exists: true,
        model_count: 0,
        total_size_bytes: 0,
      });
      vi.mocked(mockTransport.setModelsDirectory).mockResolvedValue(undefined);

      await getSystemMemory();
      await getModelsDirectory();
      await setModelsDirectory('/new/path');

      expect(mockTransport.getSystemMemory).toHaveBeenCalled();
      expect(mockTransport.getModelsDirectory).toHaveBeenCalled();
      expect(mockTransport.setModelsDirectory).toHaveBeenCalled();
    });
  });
});
