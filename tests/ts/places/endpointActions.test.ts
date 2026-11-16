/**
 * A start the endpoint place asks for shows its outcome at once: the status
 * the daemon answers the start with is the registry's, without waiting for
 * an event. The event stream here delivers nothing, so only the answer can
 * move the line.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import type { ProxyStatus } from '../../../src/services/transport/types/proxy';

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null, pinned_to: null, pinned_model_id: null };
const PINNED: ProxyStatus = { ...UP, state: 'pinned', pinned_model: 'Qwen3-4B', pinned_to: 'Pinned to Qwen3-4B', pinned_model_id: 1 };

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  startProxy: vi.fn(async (_c?: unknown): Promise<unknown> => null),
  startPinnedProxy: vi.fn(async (_r: unknown): Promise<unknown> => null),
  stopProxy: vi.fn(async () => {}),
}));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => transport,
}));

import { useEndpointActions } from '../../../src/places/endpoint/useEndpointActions';
import { getProxyState, resetProxyState } from '../../../src/services/proxyRegistry';

describe('the endpoint actions', () => {
  beforeEach(() => {
    resetProxyState();
    transport.startProxy.mockReset().mockResolvedValue(UP);
    transport.startPinnedProxy.mockReset().mockResolvedValue(PINNED);
  });

  it('Turn on takes the status the start answers as the endpoint\'s, with no event', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.turnOn());

    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });
  });

  it('serving one model takes the pinned start\'s answer', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(1));

    expect(transport.startPinnedProxy).toHaveBeenCalledWith({ model_id: 1 });
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: PINNED });
  });

  it('serving every model again takes the plain start\'s answer', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(null));

    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });
  });

  it('a refused start leaves the registry as it was', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error('port 8080 is in use'));
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.turnOn());

    expect(result.current.error).toBe('port 8080 is in use');
    expect(getProxyState()).toEqual({ running: false, port: null, status: null });
  });
});
