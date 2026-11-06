/**
 * The proxy's live dashboard, one stream per endpoint.
 *
 * `/v1/proxy/status/stream` is the proxy's own, on its own port with its own
 * credential (`services/clients/proxyDashboard`). Each surface that showed it
 * used to open its own stream: the dashboard modal one, the tray another, and
 * the redesign's sidebar words, flag lines and recent-requests list would
 * have been three more against the same endpoint. This registry opens one
 * stream for each distinct (host, port, key) while anyone wants it, counts
 * who does, and closes it when the last one leaves; a surface whose key
 * changes leaves the old stream and joins the new one.
 *
 * The tray popover is its own window, so its own copy of this module; the
 * sharing is among the surfaces of one window.
 *
 * @module services/dashboardRegistry
 */

import { useCallback, useEffect, useSyncExternalStore } from 'react';
import { createEventStore } from './createEventStore';
import { subscribeProxyDashboard } from './clients/proxyDashboard';
import type { DashboardSnapshot } from './transport/types/dashboard';

/** Where a dashboard stream is read from, and with what. */
export interface DashboardTarget {
  host: string;
  port: number;
  /** The proxy's key, or `null` when it runs without one. */
  apiKey: string | null;
}

export interface DashboardLive {
  /** Latest snapshot, or `null` before the first event has arrived. */
  snapshot: DashboardSnapshot | null;
  /** Whether the stream has delivered at least one snapshot since connecting. */
  connected: boolean;
}

const IDLE: DashboardLive = { snapshot: null, connected: false };

/** One key per endpoint-and-credential; the port makes it unique per proxy. */
function keyOf(target: DashboardTarget): string {
  return `${target.host}:${target.port}:${target.apiKey ?? ''}`;
}

/** What each stream knows, keyed as above. Replaced whole on every write. */
const store = createEventStore<ReadonlyMap<string, DashboardLive>>(new Map());

interface Handle {
  count: number;
  close: () => void;
}

const handles = new Map<string, Handle>();

function write(key: string, live: DashboardLive): void {
  store.setState(new Map(store.getState()).set(key, live));
}

function open(key: string, target: DashboardTarget): Handle {
  write(key, IDLE);
  const handle: Handle = { count: 0, close: () => {} };
  // A frame already read when the stream is closed must not bring the key
  // back: only the stream this handle holds may write under it.
  const held = () => handles.get(key) === handle;
  handle.close = subscribeProxyDashboard(
    target.host,
    target.port,
    target.apiKey,
    (snapshot) => {
      if (held()) write(key, { snapshot, connected: true });
    },
    () => {
      if (!held()) return;
      const current = store.getState().get(key) ?? IDLE;
      write(key, { ...current, connected: false });
    },
  );
  return handle;
}

/**
 * Want the stream for `target`. Opens it if nobody else has; returns the
 * release, which closes it when the last holder lets go.
 */
export function acquireDashboard(target: DashboardTarget): () => void {
  const key = keyOf(target);
  const handle = handles.get(key) ?? open(key, target);
  handles.set(key, handle);
  handle.count += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    handle.count -= 1;
    if (handle.count > 0) return;
    handle.close();
    handles.delete(key);
    const next = new Map(store.getState());
    next.delete(key);
    store.setState(next);
  };
}

/** How many streams are open now, for tests and diagnostics. */
export function openDashboardStreams(): number {
  return handles.size;
}

/**
 * React hook: the live dashboard for `target`, held open while this component
 * wants it. `null` keeps it idle, for a proxy not running or a key not yet
 * revealed, and gives back no snapshot.
 */
export function useDashboard(target: DashboardTarget | null): DashboardLive {
  const key = target === null ? null : keyOf(target);
  const host = target?.host;
  const port = target?.port;
  const apiKey = target?.apiKey;

  useEffect(() => {
    if (host === undefined || port === undefined || apiKey === undefined) return undefined;
    return acquireDashboard({ host, port, apiKey });
  }, [host, port, apiKey]);

  const read = useCallback(
    () => (key === null ? IDLE : (store.getState().get(key) ?? IDLE)),
    [key],
  );
  return useSyncExternalStore(store.subscribe, read, read);
}
