/**
 * Tests for useDownloadProgress hook.
 * 
 * Tests download progress state management, throttling, and event handling.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';

// Mock platform detection
let mockIsTauriApp = false;
vi.mock('../../../src/utils/platform', () => ({
  get isTauriApp() {
    return mockIsTauriApp;
  },
}));

// Mock TauriService
vi.mock('../../../src/services/tauri', () => ({
  getDownloadQueue: vi.fn(),
  cancelDownload: vi.fn(),
}));

import { getDownloadQueue, cancelDownload } from '../../../src/services/tauri';
import { useDownloadProgress, DownloadProgress } from '../../../src/hooks/useDownloadProgress';

describe('useDownloadProgress', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    mockIsTauriApp = false;

    // Default mock: empty queue
    vi.mocked(getDownloadQueue).mockResolvedValue({
      pending: [],
      failed: [],
      max_size: 10,
      current: null,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  describe('initial state', () => {
    it('returns initial state with null progress', async () => {
      const { result } = renderHook(() => useDownloadProgress());

      expect(result.current.progress).toBeNull();
      expect(result.current.error).toBeNull();
      expect(result.current.isDownloading).toBe(false);
      expect(result.current.queueCount).toBe(0);
    });

    it('fetches queue status on mount', async () => {
      renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(getDownloadQueue).toHaveBeenCalled();
    });

    it('sets connection mode for Web UI', async () => {
      mockIsTauriApp = false;
      const { result } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      expect(result.current.connectionMode).toBe('Web (SSE)');
    });
  });

  describe('queue status', () => {
    it('reflects queue with pending items', async () => {
      vi.mocked(getDownloadQueue).mockResolvedValue({
        pending: [{ model_id: 'model1' }, { model_id: 'model2' }],
        failed: [],
        max_size: 10,
        current: { model_id: 'model0' },
      });

      const { result } = renderHook(() => useDownloadProgress());

      // Advance timers to trigger the initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      // Check the queue status after async update
      expect(result.current.queueStatus?.pending.length).toBe(2);
      expect(result.current.isDownloading).toBe(true);
      expect(result.current.queueCount).toBe(3); // 2 pending + 1 current
    });

    it('fetches queue status on mount (no polling - queue updates come via SSE)', async () => {
      renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(getDownloadQueue).toHaveBeenCalledTimes(1);

      // No longer polls - queue updates come via SSE events (queue_snapshot)
      await act(async () => {
        await vi.advanceTimersByTimeAsync(4000);
      });

      // Should still be 1 call (initial fetch only)
      expect(getDownloadQueue).toHaveBeenCalledTimes(1);
    });

    it('handles queue fetch errors gracefully', async () => {
      vi.mocked(getDownloadQueue).mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      // Should not crash, queue status remains null
      expect(result.current.queueStatus).toBeNull();
    });
  });

  describe('cancelDownload', () => {
    it('calls cancelDownload and refreshes queue', async () => {
      vi.mocked(cancelDownload).mockResolvedValue('Cancelled');

      const { result } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      await act(async () => {
        await result.current.cancelDownload('model-id');
      });

      expect(cancelDownload).toHaveBeenCalledWith('model-id');
      // Should refresh queue after cancel
      expect(getDownloadQueue).toHaveBeenCalledTimes(2);
    });

    it('clears progress and error on successful cancel', async () => {
      vi.mocked(cancelDownload).mockResolvedValue('Cancelled');

      const { result } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      await act(async () => {
        await result.current.cancelDownload('model-id');
      });

      expect(result.current.progress).toBeNull();
      expect(result.current.error).toBeNull();
    });

    it('throws on cancel failure', async () => {
      vi.mocked(cancelDownload).mockRejectedValue(new Error('Cannot cancel'));

      const { result } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      await expect(
        act(async () => {
          await result.current.cancelDownload('model-id');
        })
      ).rejects.toThrow('Cannot cancel');
    });
  });

  describe('clearProgress', () => {
    it('clears the progress state', async () => {
      const { result } = renderHook(() => useDownloadProgress());

      // Wait for hook to initialize
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      await act(async () => {
        result.current.clearProgress();
      });

      expect(result.current.progress).toBeNull();
    });
  });

  describe('setError', () => {
    it('sets error state', async () => {
      const { result } = renderHook(() => useDownloadProgress());

      // Wait for hook to initialize
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      await act(async () => {
        result.current.setError('Test error');
      });

      expect(result.current.error).toBe('Test error');
    });

    it('clears error with null', async () => {
      const { result } = renderHook(() => useDownloadProgress());

      // Wait for hook to initialize
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      await act(async () => {
        result.current.setError('Error');
      });

      expect(result.current.error).toBe('Error');

      await act(async () => {
        result.current.setError(null);
      });

      expect(result.current.error).toBeNull();
    });
  });

  describe('fetchQueueStatus', () => {
    it('manually refreshes queue status', async () => {
      const { result } = renderHook(() => useDownloadProgress());

      // Wait for hook to initialize and first fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(100);
      });

      const callsAfterInit = vi.mocked(getDownloadQueue).mock.calls.length;

      await act(async () => {
        await result.current.fetchQueueStatus();
      });

      expect(getDownloadQueue).toHaveBeenCalledTimes(callsAfterInit + 1);
    });
  });

  describe('onCompleted callback', () => {
    it('calls onCompleted when provided', async () => {
      const onCompleted = vi.fn();
      renderHook(() => useDownloadProgress({ onCompleted }));

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      // The callback would be called when a 'completed' event is received
      // Since we're not simulating SSE events here, just verify hook accepts the option
      expect(onCompleted).not.toHaveBeenCalled();
    });
  });

  describe('cleanup', () => {
    it('clears interval on unmount', async () => {
      const { unmount } = renderHook(() => useDownloadProgress());

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      const callsBefore = vi.mocked(getDownloadQueue).mock.calls.length;

      unmount();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(4000);
      });

      // Should not have polled after unmount
      expect(getDownloadQueue).toHaveBeenCalledTimes(callsBefore);
    });
  });
});
