/**
 * useProxyDashboard hook.
 *
 * Subscribes a component to a running proxy's live dashboard stream for as
 * long as it is mounted, via a native `EventSource`
 * (`services/clients/proxyDashboard.ts`) — not the app's internal
 * multiplexed SSE bus (`transport/events/sse.ts`), since the dashboard lives
 * on the proxy's own arbitrary host:port, not the app's own backend.
 *
 * @module hooks/useProxyDashboard
 */

import { useEffect, useState } from 'react';
import { subscribeProxyDashboard } from '../services/clients/proxyDashboard';
import type { DashboardSnapshot } from '../services/transport/types/dashboard';

export interface UseProxyDashboardOptions {
  /** Proxy host, e.g. "127.0.0.1". */
  host: string;
  /** Proxy port. Pass `null` to stay idle (e.g. proxy not running yet). */
  port: number | null;
}

export interface UseProxyDashboardResult {
  /** Latest snapshot, or `null` before the first event has arrived. */
  snapshot: DashboardSnapshot | null;
  /** Whether the stream has delivered at least one snapshot since connecting. */
  connected: boolean;
}

export function useProxyDashboard({ host, port }: UseProxyDashboardOptions): UseProxyDashboardResult {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    if (port == null) {
      setSnapshot(null);
      setConnected(false);
      return;
    }

    setSnapshot(null);
    setConnected(false);

    const unsubscribe = subscribeProxyDashboard(
      host,
      port,
      (next) => {
        setSnapshot(next);
        setConnected(true);
      },
      () => {
        setConnected(false);
      }
    );

    // Explicit cleanup: close the underlying EventSource on unmount or when
    // host/port changes, so we never leak connections or leave duplicates
    // open across re-renders.
    return () => {
      unsubscribe();
    };
  }, [host, port]);

  return { snapshot, connected };
}
