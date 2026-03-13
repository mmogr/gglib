/**
 * Tests for useServers hook.
 *
 * useServers is event-driven (backed by serverRegistry), not polling-based.
 * Tests cover: registry state mapping, static interface contracts, and
 * delegation to safeStopServer.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useServers } from '../../../src/hooks/useServers';
import type { ServerStateInfo } from '../../../src/services/serverRegistry';
import { MOCK_BASE_PORT } from '../fixtures/ports';

// useAllServerStates is the sole data source — mock the registry.
vi.mock('../../../src/services/serverRegistry', () => ({
  useAllServerStates: vi.fn(),
}));

// Mock safe actions for stopServer delegation.
vi.mock('../../../src/services/server/safeActions', () => ({
  safeStopServer: vi.fn(),
}));

import { useAllServerStates } from '../../../src/services/serverRegistry';
import { safeStopServer } from '../../../src/services/server/safeActions';

const mockRegistryState: ServerStateInfo[] = [
  { modelId: '1', modelName: 'llama-7b', port: MOCK_BASE_PORT, status: 'running', updatedAt: 1 },
  { modelId: '2', modelName: 'mistral-7b', port: MOCK_BASE_PORT + 1, status: 'running', updatedAt: 2 },
];

describe('useServers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useAllServerStates).mockReturnValue(mockRegistryState);
    vi.mocked(safeStopServer).mockResolvedValue(undefined);
  });

  it('returns servers mapped from registry state', () => {
    const { result } = renderHook(() => useServers());

    expect(result.current.servers).toEqual([
      { modelId: 1, modelName: 'llama-7b', port: MOCK_BASE_PORT, status: 'running' },
      { modelId: 2, modelName: 'mistral-7b', port: MOCK_BASE_PORT + 1, status: 'running' },
    ]);
  });

  it('loading is always false (event-driven, no async fetch)', () => {
    const { result } = renderHook(() => useServers());
    expect(result.current.loading).toBe(false);
  });

  it('error is always null (errors handled by registry, not hook)', () => {
    const { result } = renderHook(() => useServers());
    expect(result.current.error).toBeNull();
  });

  it('reflects updated registry state on re-render', () => {
    const updated: ServerStateInfo[] = [
      { modelId: '3', modelName: 'gemma-7b', port: MOCK_BASE_PORT + 2, status: 'running', updatedAt: 3 },
    ];

    const { result, rerender } = renderHook(() => useServers());
    expect(result.current.servers).toHaveLength(2);

    vi.mocked(useAllServerStates).mockReturnValue(updated);
    rerender();

    expect(result.current.servers).toHaveLength(1);
    expect(result.current.servers[0].modelId).toBe(3);
  });

  it('stopServer delegates to safeStopServer', async () => {
    const { result } = renderHook(() => useServers());

    await act(async () => {
      await result.current.stopServer(1);
    });

    expect(safeStopServer).toHaveBeenCalledWith(1);
    expect(safeStopServer).toHaveBeenCalledTimes(1);
  });

  it('loadServers is a no-op that resolves cleanly', async () => {
    const { result } = renderHook(() => useServers());

    // loadServers is intentionally a no-op; the registry is event-driven.
    await act(async () => {
      await result.current.loadServers();
    });

    expect(vi.mocked(useAllServerStates)).toHaveBeenCalled();
  });

  it('handles empty server list', () => {
    vi.mocked(useAllServerStates).mockReturnValue([]);

    const { result } = renderHook(() => useServers());

    expect(result.current.servers).toEqual([]);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });
});
