/**
 * Server logs utilities
 * TRANSPORT_EXCEPTION: Uses Tauri invoke/events for server log streaming.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

export interface ServerLogEntry {
  timestamp: number;
  line: string;
  port: number;
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
  const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
  const response = await fetch(`${baseUrl}/api/servers/${port}/logs`);
  if (response.ok) {
    const json = await response.json() as { success: boolean; data?: { logs: ServerLogEntry[] } };
    return json.data?.logs ?? [];
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
  const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
  const eventSource = new EventSource(`${baseUrl}/api/servers/${port}/logs/stream`);
  
  eventSource.onmessage = (event) => {
    try {
      if (!event.data || event.data.trim() === '') return;
      const logEntry = JSON.parse(event.data) as ServerLogEntry;
      callback(logEntry);
    } catch (e) {
      console.error('[serverLogs] Failed to parse log event:', e);
    }
  };
  
  eventSource.onerror = (err) => {
    console.error('[serverLogs] SSE Error:', err);
  };
  
  return () => eventSource.close();
}
