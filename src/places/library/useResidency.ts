/**
 * The endpoint's live dashboard as this window reads it, and what each
 * loaded model is doing in it.
 *
 * `useEndpointDashboard` is the one place the dashboard target is built:
 * the host of the status's `base_url` (the daemon's dial rule, so an
 * endpoint bound to one interface is reached there), the running port, and
 * the proxy's key once revealed; `dashboardRegistry` keeps one stream for
 * it however many surfaces read it. Idle while the endpoint is off or its
 * key not yet revealed.
 *
 * `useResidency` is the residency word the stream gives each resident (its
 * admission slots, by model id). The server list's `launch.state` is the
 * same word read once, which a request starting or ending would leave
 * stale; the stream moves with them. Empty while the dashboard is idle: a
 * loaded row then says Loaded.
 *
 * @module places/library/useResidency
 */

import { useMemo } from 'react';
import { useProxyKey } from '../../hooks/useProxyKey';
import { useDashboard, type DashboardLive, type DashboardTarget } from '../../services/dashboardRegistry';
import { useProxyState } from '../../services/proxyRegistry';
import type { ResidencyState } from '../../types/generated/ResidencyState';

/**
 * Where this app reaches the endpoint: the host of the status's `base_url`,
 * and loopback until the status has been read.
 */
function dialHost(baseUrl: string | null | undefined): string {
  if (!baseUrl) return '127.0.0.1';
  try {
    return new URL(baseUrl).hostname;
  } catch {
    return '127.0.0.1';
  }
}

export interface EndpointDashboard extends DashboardLive {
  /** The host the stream is read from. */
  host: string;
  /** The running port; `null` while the endpoint is off. */
  port: number | null;
  /** The proxy's key: `undefined` until revealed, `null` when there is none. */
  apiKey: string | null | undefined;
}

export function useEndpointDashboard(): EndpointDashboard {
  const proxy = useProxyState();
  const apiKey = useProxyKey(proxy.running);
  const host = dialHost(proxy.status?.base_url);
  const port = proxy.running ? proxy.port : null;
  const target: DashboardTarget | null = port !== null && apiKey !== undefined ? { host, port, apiKey } : null;
  return { ...useDashboard(target), host, port, apiKey };
}

export function useResidency(): ReadonlyMap<number, ResidencyState> {
  const { snapshot } = useEndpointDashboard();
  return useMemo(() => {
    const states = new Map<number, ResidencyState>();
    for (const slot of snapshot?.admission?.slots ?? []) {
      if (slot.launch) states.set(slot.model_id, slot.launch.state);
    }
    return states;
  }, [snapshot]);
}
