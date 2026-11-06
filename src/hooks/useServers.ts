import { useCallback } from 'react';
import { useAllServerStates, type ServerStatus } from '../services/serverRegistry';
import { safeStopServer } from '../services/server/safeActions';

/**
 * A running server as the UI draws it.
 *
 * Built here from the event registry rather than fetched, and deliberately
 * not `ServerInfo`: `status` has no counterpart on the wire at all — it is
 * registry state — and `modelName` falls back to a synthesized label when the
 * registry has not learned one. The two shapes answer different questions,
 * so they are two types.
 */
export interface ServerViewModel {
  modelId: number;
  modelName: string;
  port: number;
  status: ServerStatus;
}

/**
 * Hook providing running server list from the event-driven registry.
 *
 * State is kept current by server lifecycle events flowing through
 * serverRegistry, so there is nothing to poll and nothing to refresh.
 */
export function useServers() {
  const serverStates = useAllServerStates();

  const servers: ServerViewModel[] = serverStates.map((s) => {
    const modelId = Number(s.modelId);
    // Never render a stringified missing id ("Model undefined") — fall back
    // through the most specific identity we actually have.
    const fallbackName = Number.isFinite(modelId)
      ? `Model #${modelId}`
      : s.port
        ? `Server :${s.port}`
        : 'Unknown server';
    return {
      modelId,
      modelName: s.modelName ?? fallbackName,
      port: s.port ?? 0,
      status: s.status,
    };
  });

  const stopServer = useCallback(async (modelId: number) => {
    await safeStopServer(modelId);
  }, []);

  return { servers, stopServer };
}
