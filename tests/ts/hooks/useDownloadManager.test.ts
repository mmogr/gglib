import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useDownloadManager } from '../../../src/hooks/useDownloadManager';

// Mock platform detection to force web/SSE mode
vi.mock('../../../src/utils/platform', () => ({
  isTauriApp: false,
}));

// Transport mock: queue operations plus the (sync) event subscription.
let mockSubscribeHandler: ((event: any) => void) | null = null;
let unsubscribeCalled = false;

const transport = vi.hoisted(() => ({
  queueDownload: vi.fn(),
  getDownloadQueue: vi.fn(),
  cancelDownload: vi.fn(),
  cancelShardGroup: vi.fn(),
  clearFailedDownloads: vi.fn(),
  subscribe: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

const mockQueueDownload = transport.queueDownload;
const mockCancelDownload = transport.cancelDownload;
const mockCancelShardGroup = transport.cancelShardGroup;
const mockClearFailedDownloads = transport.clearFailedDownloads;
const mockGetDownloadQueue = transport.getDownloadQueue;
const mockSubscribeToEvent = transport.subscribe;

mockSubscribeToEvent.mockImplementation((_eventType: string, handler: (event: any) => void) => {
  mockSubscribeHandler = handler;
  return () => {
    unsubscribeCalled = true;
  };
});

const emptySnapshot = { current: null, pending: [], failed: [], max_size: 3 };

describe('useDownloadManager', () => {
  beforeEach(() => {
    unsubscribeCalled = false;
    mockSubscribeHandler = null;
    mockQueueDownload.mockReset();
    mockCancelDownload.mockReset();
    mockCancelShardGroup.mockReset();
    mockClearFailedDownloads.mockReset();
    mockSubscribeToEvent.mockClear();
    mockGetDownloadQueue.mockReset();
    mockGetDownloadQueue.mockResolvedValue(emptySnapshot);
  });

  it('initializes queue and sets connection mode', async () => {
    const { result } = renderHook(() => useDownloadManager());

    await waitFor(() => expect(mockGetDownloadQueue).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(result.current.queueStatus).toEqual(emptySnapshot));
    await waitFor(() => expect(result.current.connectionMode).toBe('Web (SSE)'));
  });

  it('handles progress and completion events', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { result } = renderHook(() => useDownloadManager({ onCompleted: vi.fn() }));
    await waitFor(() => expect(mockSubscribeHandler).toBeTruthy());

    act(() => {
      mockSubscribeHandler?.({
        type: 'download',
        event: {
          type: 'download_progress',
          id: 'model1',
          downloaded: 10,
          total: 100,
          speed_bps: 5,
          eta_seconds: 18,
          percentage: 10,
        },
      });
    });

    await waitFor(() => expect(result.current.currentProgress?.status).toBe('progress'));
    expect(result.current.currentProgress?.percentage).toBe(10);

    act(() => {
      mockSubscribeHandler?.({
        type: 'download',
        event: { type: 'download_completed', id: 'model1', message: 'done' },
      });
    });

    await act(async () => {
      vi.advanceTimersByTime(2000);
    });

    await waitFor(() => expect(result.current.currentProgress).toBeNull());
    vi.useRealTimers();
  });

  it('queues model and refreshes snapshot', async () => {
    let snapshot = emptySnapshot;
    mockGetDownloadQueue.mockImplementation(async () => snapshot);
    mockQueueDownload.mockResolvedValue('m2:q4');

    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(result.current.queueStatus).toEqual(emptySnapshot));

    snapshot = {
      current: { id: 'm1', status: 'downloading', message: undefined },
      pending: [{ id: 'm2', status: 'queued' }],
      failed: [],
      max_size: 3,
    } as any;

    await act(async () => {
      await result.current.queueModel('m2', 'q4');
    });

    await waitFor(() => expect(mockQueueDownload).toHaveBeenCalledWith({ modelId: 'm2', quantization: 'q4' }));
    expect(result.current.queueStatus?.pending?.length).toBe(1);
    expect(result.current.queueLength).toBe(1); // pending only (activeId set via SSE, not queue refresh)
  });

  it('cleans up subscription on unmount', async () => {
    const { unmount } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(mockSubscribeHandler).toBeTruthy());
    unmount();
    expect(unsubscribeCalled).toBe(true);
  });
});
