/**
 * One dashboard stream per endpoint: opened by the first surface that wants
 * it, shared by the next, closed by the last to leave, and reopened when the
 * key changes. The stream itself (`clients/proxyDashboard`) is held by its
 * own test; here it is a fake that records what was opened and closed.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import type { DashboardSnapshot } from '../../../src/services/transport/types/dashboard';
import { dashboardSnapshot } from '../fixtures/dashboard';

type OnSnapshot = (snapshot: DashboardSnapshot) => void;
type OnError = (error: unknown) => void;

const streams = vi.hoisted(() => ({
  opened: [] as { host: string; port: number; apiKey: string | null | undefined; onSnapshot: OnSnapshot; onError?: OnError }[],
  closed: 0,
}));

vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (host: string, port: number, apiKey: string | null | undefined, onSnapshot: OnSnapshot, onError?: OnError) => {
    streams.opened.push({ host, port, apiKey, onSnapshot, onError });
    return () => {
      streams.closed += 1;
    };
  },
}));

import { acquireDashboard, openDashboardStreams, useDashboard } from '../../../src/services/dashboardRegistry';

const LOOPBACK = { host: '127.0.0.1', port: 8080, apiKey: null };

describe('the dashboard registry', () => {
  beforeEach(() => {
    streams.opened = [];
    streams.closed = 0;
  });

  it('opens one stream for two holders of the same endpoint, and closes it when the last leaves', () => {
    const first = acquireDashboard(LOOPBACK);
    const second = acquireDashboard({ ...LOOPBACK });
    expect(streams.opened).toHaveLength(1);
    expect(openDashboardStreams()).toBe(1);

    first();
    expect(streams.closed).toBe(0);
    second();
    second();

    expect(streams.closed).toBe(1);
    expect(openDashboardStreams()).toBe(0);
  });

  it('a hook holds the stream while mounted, sees each snapshot, and lets go on unmount', () => {
    const { result, unmount } = renderHook(() => useDashboard(LOOPBACK));
    expect(result.current).toEqual({ snapshot: null, connected: false });
    expect(streams.opened).toHaveLength(1);

    const snapshot = dashboardSnapshot({ total_requests: 7 });
    act(() => streams.opened[0].onSnapshot(snapshot));
    expect(result.current).toEqual({ snapshot, connected: true });

    act(() => streams.opened[0].onError?.(new Error('dropped')));
    expect(result.current).toEqual({ snapshot, connected: false });

    unmount();
    expect(streams.closed).toBe(1);
  });

  it('a key change leaves the old stream and opens one with the new key', () => {
    const { rerender } = renderHook(({ key }: { key: string | null }) => useDashboard({ ...LOOPBACK, apiKey: key }), {
      initialProps: { key: null as string | null },
    });
    expect(streams.opened[0].apiKey).toBeNull();

    rerender({ key: 'k3y-5f3a' });

    expect(streams.closed).toBe(1);
    expect(streams.opened).toHaveLength(2);
    expect(streams.opened[1].apiKey).toBe('k3y-5f3a');
    expect(openDashboardStreams()).toBe(1);
  });

  it('two holders of different endpoints open two streams, and each reads its own', () => {
    const keyed = { ...LOOPBACK, apiKey: 'k3y-5f3a' };
    const plain = renderHook(() => useDashboard(LOOPBACK));
    const withKey = renderHook(() => useDashboard(keyed));
    expect(streams.opened.map((s) => s.apiKey)).toEqual([null, 'k3y-5f3a']);
    expect(openDashboardStreams()).toBe(2);

    const snapshot = dashboardSnapshot({ total_requests: 9 });
    act(() => streams.opened[1].onSnapshot(snapshot));

    expect(withKey.result.current.snapshot).toBe(snapshot);
    expect(plain.result.current.snapshot).toBeNull();
    plain.unmount();
    withKey.unmount();
    expect(openDashboardStreams()).toBe(0);
  });

  it('a frame that arrives after the last release writes nothing', () => {
    const release = acquireDashboard(LOOPBACK);
    const stream = streams.opened[0];
    release();

    act(() => stream.onSnapshot(dashboardSnapshot({ total_requests: 4 })));

    const { result } = renderHook(() => useDashboard(null));
    expect(result.current).toEqual({ snapshot: null, connected: false });
    const late = renderHook(() => useDashboard(LOOPBACK));
    expect(late.result.current.snapshot).toBeNull();
    late.unmount();
  });

  it('null keeps the hook idle, with no stream and no snapshot', () => {
    const { result } = renderHook(() => useDashboard(null));

    expect(result.current).toEqual({ snapshot: null, connected: false });
    expect(streams.opened).toHaveLength(0);
  });

  it('a second holder joining late sees the snapshot already received', () => {
    const release = acquireDashboard(LOOPBACK);
    const snapshot = dashboardSnapshot({ total_requests: 3 });
    act(() => streams.opened[0].onSnapshot(snapshot));

    const { result, unmount } = renderHook(() => useDashboard(LOOPBACK));

    expect(result.current.snapshot).toBe(snapshot);
    unmount();
    release();
    expect(openDashboardStreams()).toBe(0);
  });
});
