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
  getProxyStatus: vi.fn(async (): Promise<unknown> => null),
  onEventStreamOpen: vi.fn(() => () => {}),
  onEventStreamClose: vi.fn(() => () => {}),
  isEventStreamOpen: vi.fn(() => false),
}));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => transport,
}));

import { OLDER_SERVICE, useEndpointActions } from '../../../src/places/endpoint/useEndpointActions';
import { cleanupProxyEvents, initProxyEvents } from '../../../src/services/proxyEvents';
import { getProxyState, resetProxyState } from '../../../src/services/proxyRegistry';

describe('the endpoint actions', () => {
  beforeEach(() => {
    resetProxyState();
    transport.startProxy.mockReset().mockResolvedValue(UP);
    transport.startPinnedProxy.mockReset().mockResolvedValue(PINNED);
    transport.stopProxy.mockReset();
  });

  it('Turn on takes the status the start answers as the endpoint\'s, with no event', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.turnOn());

    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });
  });

  it('serving one model is one call that replaces the endpoint, and takes its answer', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(1));

    expect(transport.startPinnedProxy).toHaveBeenCalledWith({ model_id: 1, proxy: { replace: true } });
    expect(transport.stopProxy).not.toHaveBeenCalled();
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: PINNED });
  });

  it('serving every model again is one call that replaces the endpoint, and takes its answer', async () => {
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(null));

    expect(transport.startProxy).toHaveBeenCalledWith({ replace: true });
    expect(transport.stopProxy).not.toHaveBeenCalled();
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });
  });

  it('a status read asked before the start, and answered after it, does not write over the start\'s answer', async () => {
    let answerRead: (status: ProxyStatus) => void = () => {};
    transport.getProxyStatus.mockReturnValueOnce(new Promise<ProxyStatus>((resolve) => (answerRead = resolve)));
    initProxyEvents();
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.turnOn());
    await act(async () => {
      answerRead({ ...UP, running: false, state: 'off', port: null, base_url: null });
      await Promise.resolve();
    });

    expect(getProxyState()).toEqual({ running: true, port: 8080, status: UP });
    cleanupProxyEvents();
  });

  it('an older service that answers an unpin still pinned is said so, with what to do', async () => {
    // A service older than the app ignores `replace` and answers the endpoint as it was.
    transport.startProxy.mockResolvedValueOnce(PINNED);
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(null));

    expect(result.current.error).toBe(
      'The gglib service running is older than this app and did not switch the endpoint. Stop gglib… in Settings › About, then Start gglib.',
    );
    expect(result.current.error).toBe(OLDER_SERVICE);
    expect(transport.stopProxy).not.toHaveBeenCalled();
    expect(getProxyState()).toEqual({ running: true, port: 8080, status: PINNED });
  });

  it('an older service that answers a pin unpinned is said so too', async () => {
    transport.startPinnedProxy.mockResolvedValueOnce(UP);
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(1));

    expect(result.current.error).toBe(OLDER_SERVICE);
  });

  it('an older service refusing a pin is said in its own words', async () => {
    transport.startPinnedProxy.mockRejectedValueOnce(
      new Error('the proxy is already running unpinned — stop it first (`gglib proxy stop`)'),
    );
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.serve(1));

    expect(result.current.error).toBe('the proxy is already running unpinned — stop it first (`gglib proxy stop`)');
    expect(transport.stopProxy).not.toHaveBeenCalled();
  });

  it('a refused start leaves the registry as it was', async () => {
    transport.startProxy.mockRejectedValueOnce(new Error('port 8080 is in use'));
    const { result } = renderHook(() => useEndpointActions());

    await act(() => result.current.turnOn());

    expect(result.current.error).toBe('port 8080 is in use');
    expect(getProxyState()).toEqual({ running: false, port: null, status: null });
  });
});
