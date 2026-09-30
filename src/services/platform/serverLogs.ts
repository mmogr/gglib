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
import { createSSEStream } from '../../utils/sse';

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

/** How long to wait before reopening a log stream that dropped. */
const RECONNECT_DELAY_MS = 2000;

/**
 * Listen for real-time server log events.
 * Returns an unsubscribe function.
 *
 * A `fetch` stream rather than `EventSource`, which cannot send the
 * `Authorization` header every `/api` route asks for. It reopens a stream
 * that drops, as `EventSource` did, until unsubscribed.
 */
export async function listenToServerLogs(
  port: number,
  callback: (entry: ServerLogEntry) => void
): Promise<() => void> {
  const url = `${getApiBaseUrl()}/api/servers/${port}/logs/stream`;
  const controller = new AbortController();

  void (async () => {
    while (!controller.signal.aborted) {
      try {
        for await (const message of createSSEStream(url, {
          headers: getAuthHeaders(),
          signal: controller.signal,
        })) {
          if (!message.data || message.data.trim() === '' || message.data === 'ping') continue;
          try {
            callback(JSON.parse(message.data) as ServerLogEntry);
          } catch (e) {
            appLogger.error('service.server', 'Failed to parse log event', { error: e, data: message.data });
          }
        }
      } catch (err) {
        if (controller.signal.aborted) return;
        appLogger.error('service.server', 'SSE Error', { error: err, port });
      }
      if (controller.signal.aborted) return;
      await new Promise((resolve) => setTimeout(resolve, RECONNECT_DELAY_MS));
    }
  })();

  return () => controller.abort();
}
