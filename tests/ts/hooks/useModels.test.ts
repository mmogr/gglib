/**
 * Tests for useModels hook.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useModels } from '../../../src/hooks/useModels';
import { GgufModel } from '../../../src/types';

// Mock the clients/models service functions
vi.mock('../../../src/services/clients/models', () => ({
  listModels: vi.fn(),
  addModel: vi.fn(),
  removeModel: vi.fn(),
  updateModel: vi.fn(),
}));

// Mock the desktop-specific setSelectedModel (now in services/platform)
vi.mock('../../../src/services/platform', () => ({
  setSelectedModel: vi.fn().mockResolvedValue(undefined),
}));

import {
  listModels,
  addModel,
  removeModel,
  updateModel,
} from '../../../src/services/clients/models';
import { setSelectedModel } from '../../../src/services/platform';

const mockModels: GgufModel[] = [
  {
    id: 1,
    name: 'llama-7b',
    file_path: '/models/llama-7b.gguf',
    param_count_b: 7.0,
    architecture: 'llama',
    quantization: 'Q4_K_M',
    context_length: 4096,
    added_at: '2024-01-01T00:00:00Z',
  },
  {
    id: 2,
    name: 'mistral-7b',
    file_path: '/models/mistral-7b.gguf',
    param_count_b: 7.0,
    architecture: 'mistral',
    quantization: 'Q5_K_S',
    context_length: 8192,
    added_at: '2024-01-02T00:00:00Z',
  },
];

describe('useModels', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listModels).mockResolvedValue(mockModels);
  });

  it('loads models on mount', async () => {
    const { result } = renderHook(() => useModels());

    // Initially loading
    expect(result.current.loading).toBe(true);
    expect(result.current.models).toEqual([]);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.models).toEqual(mockModels);
    expect(result.current.error).toBeNull();
    expect(listModels).toHaveBeenCalledTimes(1);
  });

  it('handles error when loading models fails', async () => {
    const error = new Error('Network error');
    vi.mocked(listModels).mockRejectedValue(error);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe('Failed to load models: Network error');
    expect(result.current.models).toEqual([]);
  });

  it('selects a model by id', async () => {
    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    act(() => {
      result.current.selectModel(1);
    });

    expect(result.current.selectedModelId).toBe(1);
    expect(result.current.selectedModel).toEqual(mockModels[0]);
  });

  it('clears selection when selecting null', async () => {
    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    act(() => {
      result.current.selectModel(1);
    });

    expect(result.current.selectedModel).not.toBeNull();

    act(() => {
      result.current.selectModel(null);
    });

    expect(result.current.selectedModelId).toBeNull();
    expect(result.current.selectedModel).toBeNull();
  });

  it('returns null for selectedModel when id not found', async () => {
    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    act(() => {
      result.current.selectModel(999);
    });

    expect(result.current.selectedModelId).toBe(999);
    expect(result.current.selectedModel).toBeNull();
  });

  it('adds a model and reloads the list', async () => {
    vi.mocked(addModel).mockResolvedValue(mockModels[0]);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.addModel('/path/to/new-model.gguf');
    });

    expect(addModel).toHaveBeenCalledWith({ filePath: '/path/to/new-model.gguf' });
    // Should have reloaded models
    expect(listModels).toHaveBeenCalledTimes(2);
  });

  it('removes a model and reloads the list', async () => {
    vi.mocked(removeModel).mockResolvedValue(undefined);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.removeModel(1, false);
    });

    expect(removeModel).toHaveBeenCalledWith(1);
    expect(listModels).toHaveBeenCalledTimes(2);
  });

  it('clears selection when removing selected model', async () => {
    vi.mocked(removeModel).mockResolvedValue(undefined);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    act(() => {
      result.current.selectModel(1);
    });

    expect(result.current.selectedModelId).toBe(1);

    await act(async () => {
      await result.current.removeModel(1, false);
    });

    expect(result.current.selectedModelId).toBeNull();
  });

  it('keeps selection when removing different model', async () => {
    vi.mocked(removeModel).mockResolvedValue(undefined);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    act(() => {
      result.current.selectModel(1);
    });

    await act(async () => {
      await result.current.removeModel(2, false);
    });

    // Selection should be preserved (though model may not exist after reload)
    expect(result.current.selectedModelId).toBe(1);
  });

  it('updates a model and reloads the list', async () => {
    const updatedModel = { ...mockModels[0], name: 'updated-name' };
    vi.mocked(updateModel).mockResolvedValue(updatedModel);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.updateModel(1, { name: 'updated-name' });
    });

    expect(updateModel).toHaveBeenCalledWith({ id: 1, name: 'updated-name' });
    expect(listModels).toHaveBeenCalledTimes(2);
  });

  it('can force remove a model', async () => {
    vi.mocked(removeModel).mockResolvedValue(undefined);

    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.removeModel(1, true);
    });

    // Note: force param is now ignored by Transport - caller handles confirmation
    expect(removeModel).toHaveBeenCalledWith(1);
  });

  it('manually reloads models', async () => {
    const { result } = renderHook(() => useModels());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(listModels).toHaveBeenCalledTimes(1);

    await act(async () => {
      await result.current.loadModels();
    });

    expect(listModels).toHaveBeenCalledTimes(2);
  });
});
