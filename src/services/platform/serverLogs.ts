/**
 * Server logs utilities.
 *
 * Logs live on the gglib daemon in every mode, so both the snapshot and the
 * live stream go over HTTP/SSE — the desktop WebView uses the same endpoints
 * a browser tab does.
 *
 * The base URL comes from the transport client, which is the one thing that
 * knows where this session's daemon actually is. A second helper used to live
 * in `src/config/api.ts` returning `''` in production builds — correct for a
 * browser tab served by the daemon, and wrong for the desktop app, where a
 * relative path resolves against the WebView's own origin rather than
 * 127.0.0.1:9887. That made these two functions the only ones in the app that
 * silently failed in a packaged build and worked in `npm run dev`.
 */

import { appLogger } from './index';
import { getApiBaseUrl, getAuthHeaders } from '../transport/api/client';

export interface ServerLogEntry {
  timestamp: number;
  line: string;
  port: number;
}

function normalizeServerLogSnapshot(payload: unknown): ServerLogEntry[] {
  if (Array.isArray(payload)) {
    return payload as ServerLogEntry[];
  }

  if (payload && typeof payload === 'object') {
    const obj = payload as Record<string, unknown>;

    // Preferred Axum shape: { logs: ServerLogEntry[] }
    if (Array.isArray(obj.logs)) {
      return obj.logs as ServerLogEntry[];
    }

    // Legacy/enveloped shape: { success: boolean, data?: { logs: ServerLogEntry[] } }
    const data = obj.data;
    if (data && typeof data === 'object') {
      const dataObj = data as Record<string, unknown>;
      if (Array.isArray(dataObj.logs)) {
        return dataObj.logs as ServerLogEntry[];
      }
    }
  }

  return [];
}

/**
 * Get initial server logs for a specific port.
 */
export async function getServerLogs(port: number): Promise<ServerLogEntry[]> {
  const baseUrl = getApiBaseUrl();
  const response = await fetch(`${baseUrl}/api/servers/${port}/logs`, {
    headers: getAuthHeaders(),
  });
  if (response.ok) {
    const json = await response.json();
    return normalizeServerLogSnapshot(json);
  }
  return [];
}

/**
 * Listen for real-time server log events.
 * Returns an unsubscribe function.
 */
export async function listenToServerLogs(
  port: number,
  callback: (entry: ServerLogEntry) => void
): Promise<() => void> {
  const baseUrl = getApiBaseUrl();
  const eventSource = new EventSource(`${baseUrl}/api/servers/${port}/logs/stream`);
  
  eventSource.onmessage = (event) => {
    try {
      if (!event.data || event.data.trim() === '') return;
      if (event.data === 'ping') return;
      const logEntry = JSON.parse(event.data) as ServerLogEntry;
      callback(logEntry);
    } catch (e) {
      appLogger.error('service.server', 'Failed to parse log event', { error: e, data: event.data });
    }
  };
  
  eventSource.onerror = (err) => {
    appLogger.error('service.server', 'SSE Error', { error: err, port });
  };
  
  return () => eventSource.close();
}
