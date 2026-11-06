/**
 * What each loaded model is doing, live: the residency word the proxy's
 * dashboard stream gives each resident (its admission slots, by model id),
 * from the one stream `dashboardRegistry` keeps open for the endpoint. The
 * server list's `launch.state` is the same word read once, which a request
 * starting or ending would leave stale; the stream moves with them.
 *
 * Empty while the endpoint is off or its key not yet revealed: a loaded row
 * then says Loaded.
 *
 * @module places/library/useResidency
 */

import { useMemo } from 'react';
import { useProxyKey } from '../../hooks/useProxyKey';
import { useDashboard } from '../../services/dashboardRegistry';
import { useProxyState } from '../../services/proxyRegistry';
import type { ResidencyState } from '../../types/generated/ResidencyState';

/**
 * Where this app reaches the endpoint: the host of the status's `base_url`
 * (the daemon's dial rule, so an endpoint bound to one interface is reached
 * there), and loopback until the status has been read.
 */
function dialHost(baseUrl: string | null | undefined): string {
  if (!baseUrl) return '127.0.0.1';
  try {
    return new URL(baseUrl).hostname;
  } catch {
    return '127.0.0.1';
  }
}

export function useResidency(): ReadonlyMap<number, ResidencyState> {
  const proxy = useProxyState();
  const apiKey = useProxyKey(proxy.running);
  const { snapshot } = useDashboard(
    proxy.running && proxy.port !== null && apiKey !== undefined ? { host: dialHost(proxy.status?.base_url), port: proxy.port, apiKey } : null,
  );
  return useMemo(() => {
    const states = new Map<number, ResidencyState>();
    for (const slot of snapshot?.admission?.slots ?? []) {
      if (slot.launch) states.set(slot.model_id, slot.launch.state);
    }
    return states;
  }, [snapshot]);
}
