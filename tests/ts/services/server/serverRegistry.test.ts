/**
 * The server registry: one map of servers, replaced on every write.
 *
 * It is held in the same store as the proxy and remote registries, and two
 * things rest on a write being a *new* map rather than a change to the old
 * one: the list of running servers is rebuilt only when the map is another
 * map, and React is handed the same list until then. A list built afresh on
 * every read never settles, and a map changed in place never shows.
 *
 * The registry is one module-wide store with no reset, so each test keeps to
 * model ids of its own.
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';

import {
  getServerState,
  ingestServerEvent,
  subscribe,
  useAllServerStates,
} from '../../../../src/services/serverRegistry';

describe('serverRegistry', () => {
  it('hands React the same running list until something is written, and a new one once it is', () => {
    const { result, rerender } = renderHook(() => useAllServerStates());
    const before = result.current;

    rerender();
    expect(result.current).toBe(before);

    act(() => ingestServerEvent({ type: 'running', modelId: '101', port: 9101, updatedAt: 1 }));

    expect(result.current).not.toBe(before);
    expect(result.current.find((s) => s.modelId === '101')).toMatchObject({
      status: 'running',
      port: 9101,
    });

    act(() => ingestServerEvent({ type: 'stopped', modelId: '101', updatedAt: 2 }));

    expect(result.current.find((s) => s.modelId === '101')).toBeUndefined();
  });

  // Hydration arrives as a snapshot, so this is what a page load shows.
  it('hands React the servers a snapshot brought', () => {
    const { result } = renderHook(() => useAllServerStates());
    expect(result.current.some((s) => s.modelId === '151')).toBe(false);

    act(() =>
      ingestServerEvent({
        type: 'snapshot',
        servers: [{ modelId: '151', status: 'running', port: 9151, updatedAt: 1 }],
      }),
    );

    expect(result.current.find((s) => s.modelId === '151')).toMatchObject({
      status: 'running',
      port: 9151,
    });
  });

  it('tells a listener of each write, and of none once it has let go', () => {
    const listener = vi.fn();
    const letGo = subscribe(listener);

    ingestServerEvent({ type: 'running', modelId: '201', updatedAt: 1 });
    expect(listener).toHaveBeenCalledTimes(1);

    letGo();
    ingestServerEvent({ type: 'stopped', modelId: '201', updatedAt: 2 });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('leaves every other server\'s state the very object it was', () => {
    ingestServerEvent({ type: 'running', modelId: '301', updatedAt: 1 });
    const untouched = getServerState('301');

    ingestServerEvent({ type: 'running', modelId: '302', updatedAt: 1 });

    expect(getServerState('301')).toBe(untouched);
    expect(getServerState('302')?.status).toBe('running');
  });

  it('does not let an older event overwrite a newer state', () => {
    ingestServerEvent({ type: 'stopped', modelId: '401', updatedAt: 10 });
    ingestServerEvent({ type: 'running', modelId: '401', updatedAt: 5 });

    expect(getServerState('401')?.status).toBe('stopped');
  });

  it('takes from a snapshot only the servers it is newer about, and notifies once', () => {
    ingestServerEvent({ type: 'stopped', modelId: '501', updatedAt: 10 });
    const listener = vi.fn();
    const letGo = subscribe(listener);

    ingestServerEvent({
      type: 'snapshot',
      servers: [
        { modelId: '501', status: 'running', port: 9501, updatedAt: 5 },
        { modelId: '502', status: 'running', port: 9502, updatedAt: 5 },
      ],
    });
    letGo();

    expect(getServerState('501')?.status).toBe('stopped');
    expect(getServerState('502')).toMatchObject({ status: 'running', port: 9502 });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('holds a health change only for a server it knows', () => {
    ingestServerEvent({
      type: 'server_health_changed',
      modelId: '601',
      status: { status: 'processdied' },
      updatedAt: 1,
    });
    expect(getServerState('601')).toBeUndefined();

    ingestServerEvent({ type: 'running', modelId: '602', port: 9602, updatedAt: 1 });
    ingestServerEvent({
      type: 'server_health_changed',
      modelId: '602',
      status: { status: 'processdied' },
      updatedAt: 2,
    });

    expect(getServerState('602')).toEqual({
      status: 'running',
      port: 9602,
      updatedAt: 2,
      health: { status: 'processdied' },
      modelName: undefined,
    });
  });
});
