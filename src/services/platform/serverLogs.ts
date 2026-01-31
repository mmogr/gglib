/**
 * Server logs utilities
 * TRANSPORT_EXCEPTION: Uses Tauri invoke/events for server log streaming.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { appLogger } from './index';
import { getApiBaseUrl } from '../../config/api';
import { isDesktop } from './detect';

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
  if (isDesktop()) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<ServerLogEntry[]>('get_server_logs', { port });
  }
  
  // Web mode: fetch from REST API
  const baseUrl = getApiBaseUrl();
  const response = await fetch(`${baseUrl}/api/servers/${port}/logs`);
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
  if (isDesktop()) {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<ServerLogEntry>('server-log', (event) => {
      callback(event.payload);
    });
    return unlisten;
  }
  
  // Web mode: use SSE
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
