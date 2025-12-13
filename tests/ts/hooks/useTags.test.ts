/**
 * Tests for useTags hook.
 * 
 * Tests tag loading and model tag operations.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useTags } from '../../../src/hooks/useTags';

// Mock tags client (which delegates to Transport)
vi.mock('../../../src/services/clients/tags', () => ({
  listTags: vi.fn(),
  addModelTag: vi.fn(),
  removeModelTag: vi.fn(),
  getModelTags: vi.fn(),
}));

import {
  listTags,
  addModelTag,
  removeModelTag,
  getModelTags,
} from '../../../src/services/clients/tags';

const mockTags = ['chat', 'code', 'reasoning', 'vision'];

describe('useTags', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listTags).mockResolvedValue(mockTags);
  });

  describe('initial state and loading', () => {
    it('starts with loading state', async () => {
      const { result } = renderHook(() => useTags());

      expect(result.current.loading).toBe(true);
      expect(result.current.tags).toEqual([]);
      expect(result.current.error).toBeNull();
    });

    it('loads tags on mount', async () => {
      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.tags).toEqual(mockTags);
      expect(listTags).toHaveBeenCalledTimes(1);
    });

    it('handles loading error', async () => {
      vi.mocked(listTags).mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Failed to load tags: Network error');
      expect(result.current.tags).toEqual([]);
    });

    it('handles non-Error throw', async () => {
      vi.mocked(listTags).mockRejectedValue('string error');

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.error).toContain('string error');
      });
    });
  });

  describe('loadTags', () => {
    it('reloads tags manually', async () => {
      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(listTags).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.loadTags();
      });

      expect(listTags).toHaveBeenCalledTimes(2);
    });

    it('clears error on successful reload', async () => {
      vi.mocked(listTags)
        .mockRejectedValueOnce(new Error('First error'))
        .mockResolvedValueOnce(mockTags);

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.error).toBeTruthy();
      });

      await act(async () => {
        await result.current.loadTags();
      });

      expect(result.current.error).toBeNull();
      expect(result.current.tags).toEqual(mockTags);
    });
  });

  describe('addTagToModel', () => {
    it('adds tag and refreshes list', async () => {
      vi.mocked(addModelTag).mockResolvedValue('Tag added');

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.addTagToModel(1, 'new-tag');
      });

      expect(addModelTag).toHaveBeenCalledWith(1, 'new-tag');
      expect(listTags).toHaveBeenCalledTimes(2);
    });

    it('propagates errors', async () => {
      vi.mocked(addModelTag).mockRejectedValue(new Error('Failed'));

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.addTagToModel(1, 'tag');
        })
      ).rejects.toThrow('Failed');
    });
  });

  describe('removeTagFromModel', () => {
    it('removes tag and refreshes list', async () => {
      vi.mocked(removeModelTag).mockResolvedValue('Tag removed');

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.removeTagFromModel(1, 'chat');
      });

      expect(removeModelTag).toHaveBeenCalledWith(1, 'chat');
      expect(listTags).toHaveBeenCalledTimes(2);
    });

    it('propagates errors', async () => {
      vi.mocked(removeModelTag).mockRejectedValue(new Error('Not found'));

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.removeTagFromModel(1, 'unknown');
        })
      ).rejects.toThrow('Not found');
    });
  });

  describe('getModelTags', () => {
    it('returns tags for a specific model', async () => {
      const modelTags = ['chat', 'vision'];
      vi.mocked(getModelTags).mockResolvedValue(modelTags);

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      const tags = await result.current.getModelTags(42);

      expect(getModelTags).toHaveBeenCalledWith(42);
      expect(tags).toEqual(modelTags);
    });

    it('returns empty array when model has no tags', async () => {
      vi.mocked(getModelTags).mockResolvedValue([]);

      const { result } = renderHook(() => useTags());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      const tags = await result.current.getModelTags(1);

      expect(tags).toEqual([]);
    });
  });
});
