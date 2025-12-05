/**
 * Tests for useDownloadCompletionEffects hook.
 * 
 * Tests the orchestration layer that batches download completions,
 * triggers model refresh, and dispatches aggregated toasts.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import React from 'react';
import { useDownloadCompletionEffects } from '../../../src/hooks/useDownloadCompletionEffects';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import type { DownloadCompletionInfo } from '../../../src/download/api/types';

// Mock toast context
const mockShowToast = vi.fn();
vi.mock('../../../src/contexts/ToastContext', async () => {
  const actual = await vi.importActual<typeof import('../../../src/contexts/ToastContext')>('../../../src/contexts/ToastContext');
  return {
    ...actual,
    useToastContext: () => ({
      toasts: [],
      showToast: mockShowToast,
      dismissToast: vi.fn(),
      clearToasts: vi.fn(),
    }),
  };
});

function createCompletionInfo(overrides: Partial<DownloadCompletionInfo> = {}): DownloadCompletionInfo {
  return {
    modelId: 'test/model:Q4_K_M',
    quantization: 'Q4_K_M',
    displayName: 'Test Model Q4_K_M',
    source: 'huggingface',
    ...overrides,
  };
}

describe('useDownloadCompletionEffects', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('single completion', () => {
    it('triggers refresh after window expires', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo());
      });

      expect(refreshModels).not.toHaveBeenCalled();

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(refreshModels).toHaveBeenCalledTimes(1);
    });

    it('shows toast with display name for single completion', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ displayName: 'My Model' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('Downloaded My Model', 'success');
    });

    it('falls back to modelId when displayName is missing', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ displayName: undefined, modelId: 'repo/model' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('Downloaded repo/model', 'success');
    });
  });

  describe('batched completions', () => {
    it('batches multiple completions within window into single refresh', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ modelId: 'model1' }));
      });

      act(() => {
        vi.advanceTimersByTime(30);
        result.current.onCompleted(createCompletionInfo({ modelId: 'model2' }));
      });

      act(() => {
        vi.advanceTimersByTime(30);
        result.current.onCompleted(createCompletionInfo({ modelId: 'model3' }));
      });

      expect(refreshModels).not.toHaveBeenCalled();

      act(() => {
        vi.advanceTimersByTime(40); // Total 100ms
      });

      // Key assertion: only ONE refresh for all three completions
      expect(refreshModels).toHaveBeenCalledTimes(1);
    });

    it('shows aggregated toast for multiple completions', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ modelId: 'model1' }));
        result.current.onCompleted(createCompletionInfo({ modelId: 'model2' }));
        result.current.onCompleted(createCompletionInfo({ modelId: 'model3' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('3 models downloaded', 'success');
    });

    it('shows "2 models downloaded" for exactly two completions', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ modelId: 'model1' }));
        result.current.onCompleted(createCompletionInfo({ modelId: 'model2' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('2 models downloaded', 'success');
    });
  });

  describe('spaced completions', () => {
    it('completions across windows result in separate refresh calls', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      // First completion
      act(() => {
        result.current.onCompleted(createCompletionInfo({ modelId: 'model1' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(refreshModels).toHaveBeenCalledTimes(1);

      // Second completion (new window)
      act(() => {
        result.current.onCompleted(createCompletionInfo({ modelId: 'model2' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(refreshModels).toHaveBeenCalledTimes(2);
    });

    it('shows separate toasts for completions in different windows', () => {
      const refreshModels = vi.fn();
      const { result } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo({ displayName: 'Model A' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('Downloaded Model A', 'success');

      act(() => {
        result.current.onCompleted(createCompletionInfo({ displayName: 'Model B' }));
      });

      act(() => {
        vi.advanceTimersByTime(100);
      });

      expect(mockShowToast).toHaveBeenCalledWith('Downloaded Model B', 'success');
      expect(mockShowToast).toHaveBeenCalledTimes(2);
    });
  });

  describe('cleanup on unmount', () => {
    it('disposes batcher on unmount, preventing timer from firing', () => {
      const refreshModels = vi.fn();
      const { result, unmount } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      act(() => {
        result.current.onCompleted(createCompletionInfo());
      });

      // Unmount before timer fires
      unmount();

      act(() => {
        vi.advanceTimersByTime(100);
      });

      // Should NOT call refresh after unmount
      expect(refreshModels).not.toHaveBeenCalled();
    });
  });

  describe('callback stability', () => {
    it('returns stable onCompleted callback reference', () => {
      const refreshModels = vi.fn();
      const { result, rerender } = renderHook(() =>
        useDownloadCompletionEffects({ refreshModels, windowMs: 100 })
      );

      const firstCallback = result.current.onCompleted;
      rerender();
      const secondCallback = result.current.onCompleted;

      expect(firstCallback).toBe(secondCallback);
    });
  });
});
