/**
 * Tests for models client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  listModels,
  getModel,
  addModel,
  removeModel,
  updateModel,
  searchModels,
  getModelFilterOptions,
} from '../../../../src/services/clients/models';
import { getTransport, _resetTransport } from '../../../../src/services/transport';
import type { GgufModel, ModelFilterOptions } from '../../../../src/types';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    listModels: vi.fn(),
    getModel: vi.fn(),
    addModel: vi.fn(),
    removeModel: vi.fn(),
    updateModel: vi.fn(),
    searchModels: vi.fn(),
    getModelFilterOptions: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/models', () => {
  const mockTransport = getTransport();

  // Sample mock data
  const mockModel: GgufModel = {
    id: 1,
    name: 'Test Model',
    file_path: '/path/to/model.gguf',
    file_size: 1024 * 1024 * 1024,
    quantization: 'Q4_K_M',
    parameters: 7_000_000_000,
    context_length: 4096,
    tags: ['test'],
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('listModels', () => {
    it('delegates to transport.listModels()', async () => {
      vi.mocked(mockTransport.listModels).mockResolvedValue([mockModel]);

      const result = await listModels();

      expect(mockTransport.listModels).toHaveBeenCalledTimes(1);
      expect(result).toEqual([mockModel]);
    });
  });

  describe('getModel', () => {
    it('delegates to transport.getModel()', async () => {
      vi.mocked(mockTransport.getModel).mockResolvedValue(mockModel);

      const result = await getModel(1);

      expect(mockTransport.getModel).toHaveBeenCalledWith(1);
      expect(result).toEqual(mockModel);
    });

    it('returns null for non-existent model', async () => {
      vi.mocked(mockTransport.getModel).mockResolvedValue(null);

      const result = await getModel(999);

      expect(result).toBeNull();
    });
  });

  describe('addModel', () => {
    it('delegates to transport.addModel()', async () => {
      const params = { filePath: '/path/to/new.gguf' };
      vi.mocked(mockTransport.addModel).mockResolvedValue(mockModel);

      const result = await addModel(params);

      expect(mockTransport.addModel).toHaveBeenCalledWith(params);
      expect(result).toEqual(mockModel);
    });
  });

  describe('removeModel', () => {
    it('delegates to transport.removeModel()', async () => {
      vi.mocked(mockTransport.removeModel).mockResolvedValue(undefined);

      await removeModel(1);

      expect(mockTransport.removeModel).toHaveBeenCalledWith(1);
    });
  });

  describe('updateModel', () => {
    it('delegates to transport.updateModel()', async () => {
      const params = { id: 1, name: 'Renamed Model' };
      const updatedModel = { ...mockModel, name: 'Renamed Model' };
      vi.mocked(mockTransport.updateModel).mockResolvedValue(updatedModel);

      const result = await updateModel(params);

      expect(mockTransport.updateModel).toHaveBeenCalledWith(params);
      expect(result).toEqual(updatedModel);
    });
  });

  describe('searchModels', () => {
    it('delegates to transport.searchModels()', async () => {
      const params = { query: 'llama', tags: ['chat'] };
      vi.mocked(mockTransport.searchModels).mockResolvedValue([mockModel]);

      const result = await searchModels(params);

      expect(mockTransport.searchModels).toHaveBeenCalledWith(params);
      expect(result).toEqual([mockModel]);
    });
  });

  describe('getModelFilterOptions', () => {
    it('delegates to transport.getModelFilterOptions()', async () => {
      const mockOptions: ModelFilterOptions = {
        quantizations: ['Q4_K_M', 'Q5_K_M'],
        param_range: { min: 1_000_000_000, max: 70_000_000_000 },
        context_range: { min: 2048, max: 128000 },
      };
      vi.mocked(mockTransport.getModelFilterOptions).mockResolvedValue(mockOptions);

      const result = await getModelFilterOptions();

      expect(mockTransport.getModelFilterOptions).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockOptions);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      vi.mocked(mockTransport.listModels).mockResolvedValue([]);
      vi.mocked(mockTransport.getModel).mockResolvedValue(null);
      vi.mocked(mockTransport.addModel).mockResolvedValue(mockModel);
      vi.mocked(mockTransport.removeModel).mockResolvedValue(undefined);
      vi.mocked(mockTransport.updateModel).mockResolvedValue(mockModel);
      vi.mocked(mockTransport.searchModels).mockResolvedValue([]);
      vi.mocked(mockTransport.getModelFilterOptions).mockResolvedValue({
        quantizations: [],
        param_range: { min: 0, max: 0 },
        context_range: { min: 0, max: 0 },
      });

      await listModels();
      await getModel(1);
      await addModel({ filePath: '/test.gguf' });
      await removeModel(1);
      await updateModel({ id: 1, name: 'test' });
      await searchModels({ query: 'test' });
      await getModelFilterOptions();

      expect(mockTransport.listModels).toHaveBeenCalled();
      expect(mockTransport.getModel).toHaveBeenCalled();
      expect(mockTransport.addModel).toHaveBeenCalled();
      expect(mockTransport.removeModel).toHaveBeenCalled();
      expect(mockTransport.updateModel).toHaveBeenCalled();
      expect(mockTransport.searchModels).toHaveBeenCalled();
      expect(mockTransport.getModelFilterOptions).toHaveBeenCalled();
    });
  });
});
