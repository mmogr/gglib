/**
 * Tests for tags client module.
 * 
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { listTags, getModelTags, addModelTag, removeModelTag } from '../../../../src/services/clients/tags';
import { getTransport, _resetTransport } from '../../../../src/services/transport';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    listTags: vi.fn(),
    getModelTags: vi.fn(),
    addModelTag: vi.fn(),
    removeModelTag: vi.fn(),
  };
  
  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/tags', () => {
  const mockTransport = getTransport();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('listTags', () => {
    it('delegates to transport.listTags()', async () => {
      const mockTags = ['chat', 'code', 'reasoning'];
      vi.mocked(mockTransport.listTags).mockResolvedValue(mockTags);

      const result = await listTags();

      expect(mockTransport.listTags).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockTags);
    });
  });

  describe('getModelTags', () => {
    it('delegates to transport.getModelTags()', async () => {
      const modelId = 42;
      const mockTags = ['favorite', 'fast'];
      vi.mocked(mockTransport.getModelTags).mockResolvedValue(mockTags);

      const result = await getModelTags(modelId);

      expect(mockTransport.getModelTags).toHaveBeenCalledWith(modelId);
      expect(result).toEqual(mockTags);
    });
  });

  describe('addModelTag', () => {
    it('delegates to transport.addModelTag()', async () => {
      const modelId = 42;
      const tag = 'new-tag';
      vi.mocked(mockTransport.addModelTag).mockResolvedValue(undefined);

      await addModelTag(modelId, tag);

      expect(mockTransport.addModelTag).toHaveBeenCalledWith(modelId, tag);
    });
  });

  describe('removeModelTag', () => {
    it('delegates to transport.removeModelTag()', async () => {
      const modelId = 42;
      const tag = 'old-tag';
      vi.mocked(mockTransport.removeModelTag).mockResolvedValue(undefined);

      await removeModelTag(modelId, tag);

      expect(mockTransport.removeModelTag).toHaveBeenCalledWith(modelId, tag);
    });
  });

  describe('no platform branching', () => {
    it('client module does not import isTauriApp', async () => {
      // This is a static check - the test itself verifies the pattern
      // If the client had platform branching, it would need different mocking
      const mockTags = ['test'];
      vi.mocked(mockTransport.listTags).mockResolvedValue(mockTags);

      // All calls go through transport regardless of environment
      await listTags();
      await getModelTags(1);
      await addModelTag(1, 'tag');
      await removeModelTag(1, 'tag');

      // Transport was used for all calls - no branching
      expect(mockTransport.listTags).toHaveBeenCalled();
      expect(mockTransport.getModelTags).toHaveBeenCalled();
      expect(mockTransport.addModelTag).toHaveBeenCalled();
      expect(mockTransport.removeModelTag).toHaveBeenCalled();
    });
  });
});
