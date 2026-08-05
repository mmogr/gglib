/**
 * useProxyDashboard hook.
 *
 * Subscribes a component to a running proxy's live dashboard stream for as
 * long as it is mounted, via `services/clients/proxyDashboard.ts` — not the
 * app's internal multiplexed SSE bus (`transport/events/sse.ts`), since the
 * dashboard lives on the proxy's own arbitrary host:port, not the app's own
 * backend, and carries the proxy's own credential rather than the backend's.
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
  /**
   * The proxy's API key (the `proxyApiKey` setting), when it requires one.
   *
   * `/v1/proxy/status/stream` sits behind the same bearer check as the rest
   * of `/v1/*`, so without this a key-protected proxy answers 401 and the
   * dashboard stays blank.
   */
  apiKey?: string | null;
}

export interface UseProxyDashboardResult {
  /** Latest snapshot, or `null` before the first event has arrived. */
  snapshot: DashboardSnapshot | null;
  /** Whether the stream has delivered at least one snapshot since connecting. */
  connected: boolean;
}

export function useProxyDashboard({
  host,
  port,
  apiKey,
}: UseProxyDashboardOptions): UseProxyDashboardResult {
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
      apiKey,
      (next) => {
        setSnapshot(next);
        setConnected(true);
      },
      () => {
        setConnected(false);
      }
    );

    // Explicit cleanup: abort the stream on unmount or when host/port/key
    // changes, so we never leak connections or leave duplicates open across
    // re-renders. The key is in the dependency list because a proxy that
    // gained one mid-session must be reconnected to with it.
    return () => {
      unsubscribe();
    };
  }, [host, port, apiKey]);

  return { snapshot, connected };
}
