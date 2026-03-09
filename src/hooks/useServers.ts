import { useCallback } from 'react';
import { useAllServerStates } from '../services/serverRegistry';
import { safeStopServer } from '../services/server/safeActions';
import { ServerInfo } from '../types';

/**
 * Hook providing running server list from the event-driven registry.
 *
 * Replaces the old polling hook. State is kept current by server lifecycle
 * events flowing through serverRegistry — no setInterval needed.
 *
 * `loadServers` is retained as a no-op for callers that still pass it, but
 * it is no longer necessary since the registry is event-driven.
 */
export function useServers() {
  const serverStates = useAllServerStates();

  const servers: ServerInfo[] = serverStates.map((s) => ({
    modelId: Number(s.modelId),
    modelName: s.modelName ?? `Model ${s.modelId}`,
    port: s.port ?? 0,
    status: s.status,
  }));

  const stopServer = useCallback(async (modelId: number) => {
    await safeStopServer(modelId);
  }, []);

  // No-op — registry is event-driven, manual refresh is unnecessary.
  const loadServers = useCallback(async () => {}, []);

  return {
    servers,
    loading: false,
    error: null as string | null,
    loadServers,
    stopServer,
  };
}
