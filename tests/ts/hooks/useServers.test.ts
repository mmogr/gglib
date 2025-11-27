/**
 * Tests for useServers hook.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useServers } from '../../../src/hooks/useServers';
import { ServerInfo } from '../../../src/types';

// Mock the TauriService
vi.mock('../../../src/services/tauri', () => ({
  TauriService: {
    listServers: vi.fn(),
    stopServer: vi.fn(),
  },
}));

import { TauriService } from '../../../src/services/tauri';

const mockServers: ServerInfo[] = [
  {
    model_id: 1,
    model_name: 'llama-7b',
    port: 9000,
    status: 'running',
  },
  {
    model_id: 2,
    model_name: 'mistral-7b',
    port: 9001,
    status: 'running',
  },
];

describe('useServers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.mocked(TauriService.listServers).mockResolvedValue(mockServers);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('loads servers on mount', async () => {
    vi.useRealTimers(); // Use real timers for this test

    const { result } = renderHook(() => useServers());

    // Initially loading
    expect(result.current.loading).toBe(true);
    expect(result.current.servers).toEqual([]);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.servers).toEqual(mockServers);
    expect(result.current.error).toBeNull();
    expect(TauriService.listServers).toHaveBeenCalled();
  });

  it('handles error when loading servers fails', async () => {
    vi.useRealTimers();
    const error = new Error('Connection refused');
    vi.mocked(TauriService.listServers).mockRejectedValue(error);

    const { result } = renderHook(() => useServers());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe('Failed to load servers: Connection refused');
    expect(result.current.servers).toEqual([]);
  });

  it('polls servers every 3 seconds', async () => {
    const { result } = renderHook(() => useServers());

    // Wait for initial load
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(TauriService.listServers).toHaveBeenCalledTimes(1);

    // Advance timer by 3 seconds
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });

    expect(TauriService.listServers).toHaveBeenCalledTimes(2);

    // Advance by another 3 seconds
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });

    expect(TauriService.listServers).toHaveBeenCalledTimes(3);
  });

  it('cleans up interval on unmount', async () => {
    const { unmount } = renderHook(() => useServers());

    // Wait for initial load
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(TauriService.listServers).toHaveBeenCalledTimes(1);

    // Unmount the hook
    unmount();

    // Advance timer - should not trigger more calls
    await act(async () => {
      await vi.advanceTimersByTimeAsync(6000);
    });

    // Should still be 1 call (no more after unmount)
    expect(TauriService.listServers).toHaveBeenCalledTimes(1);
  });

  it('stops a server and reloads the list', async () => {
    vi.useRealTimers();
    vi.mocked(TauriService.stopServer).mockResolvedValue('Server stopped');

    const { result } = renderHook(() => useServers());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(TauriService.listServers).mock.calls.length;

    await act(async () => {
      await result.current.stopServer(1);
    });

    expect(TauriService.stopServer).toHaveBeenCalledWith(1);
    // Should have reloaded servers
    expect(TauriService.listServers).toHaveBeenCalledTimes(callCountBefore + 1);
  });

  it('manually reloads servers', async () => {
    vi.useRealTimers();

    const { result } = renderHook(() => useServers());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(TauriService.listServers).mock.calls.length;

    await act(async () => {
      await result.current.loadServers();
    });

    expect(TauriService.listServers).toHaveBeenCalledTimes(callCountBefore + 1);
  });

  it('handles empty server list', async () => {
    vi.useRealTimers();
    vi.mocked(TauriService.listServers).mockResolvedValue([]);

    const { result } = renderHook(() => useServers());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.servers).toEqual([]);
    expect(result.current.error).toBeNull();
  });
});
