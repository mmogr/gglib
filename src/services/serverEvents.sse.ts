/**
 * SSE Event Adapter for Server Lifecycle Events (Web Mode)
 *
 * Connects to the /api/servers/events SSE endpoint and normalizes events
 * to the shared ServerEvent type, then ingests them into the server registry.
 *
 * This provides web/desktop parity for server lifecycle events.
 */

import { ingestServerEvent, type ServerEvent, type ServerStateInfo } from './serverRegistry';

let eventSource: EventSource | null = null;
let reconnectAttempt = 0;
const MAX_RECONNECT_ATTEMPTS = 5;
const RECONNECT_DELAY_MS = 1000;

/**
 * Parse an SSE event data string into a ServerEvent.
 */
function parseEvent(data: string): ServerEvent | null {
  try {
    const parsed = JSON.parse(data) as Record<string, unknown>;
    
    if (typeof parsed.type !== 'string') {
      console.warn('[serverEvents.sse] Event missing type:', parsed);
      return null;
    }

    switch (parsed.type) {
      case 'snapshot': {
        const servers = parsed.servers;
        if (!Array.isArray(servers)) {
          console.warn('[serverEvents.sse] Snapshot missing servers array');
          return null;
        }
        return {
          type: 'snapshot',
          servers: servers.map((s: Record<string, unknown>): ServerStateInfo => ({
            modelId: String(s.modelId ?? s.model_id ?? ''),
            status: (s.status as 'running') ?? 'running',
            port: typeof s.port === 'number' ? s.port : undefined,
            updatedAt: typeof s.updatedAt === 'number' ? s.updatedAt : (typeof s.updated_at === 'number' ? s.updated_at : Date.now()),
          })),
        };
      }

      case 'running':
      case 'stopping':
      case 'stopped':
      case 'crashed': {
        const modelId = String(parsed.modelId ?? parsed.model_id ?? '');
        const port = typeof parsed.port === 'number' ? parsed.port : undefined;
        const updatedAt = typeof parsed.updatedAt === 'number' ? parsed.updatedAt : (typeof parsed.updated_at === 'number' ? parsed.updated_at : Date.now());
        
        return { type: parsed.type, modelId, port, updatedAt };
      }

      default:
        console.warn('[serverEvents.sse] Unknown event type:', parsed.type);
        return null;
    }
  } catch (error) {
    console.error('[serverEvents.sse] Failed to parse event:', error);
    return null;
  }
}

/**
 * Connect to the SSE endpoint with automatic reconnection.
 */
function connect(): void {
  if (eventSource) {
    return;
  }

  const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
  const url = `${baseUrl}/api/servers/events`;

  eventSource = new EventSource(url);

  eventSource.onopen = () => {
    console.debug('[serverEvents.sse] Connected to server events');
    reconnectAttempt = 0;
  };

  eventSource.onmessage = (event) => {
    if (!event.data || event.data.trim() === '') {
      return;
    }

    const parsed = parseEvent(event.data);
    if (parsed) {
      ingestServerEvent(parsed);
    }
  };

  eventSource.onerror = (error) => {
    console.error('[serverEvents.sse] Connection error:', error);
    
    // Close the current connection
    eventSource?.close();
    eventSource = null;

    // Attempt to reconnect
    if (reconnectAttempt < MAX_RECONNECT_ATTEMPTS) {
      reconnectAttempt++;
      const delay = RECONNECT_DELAY_MS * reconnectAttempt;
      console.debug(`[serverEvents.sse] Reconnecting in ${delay}ms (attempt ${reconnectAttempt}/${MAX_RECONNECT_ATTEMPTS})`);
      setTimeout(connect, delay);
    } else {
      console.error('[serverEvents.sse] Max reconnect attempts reached');
    }
  };
}

/**
 * Initialize SSE connection for server lifecycle events.
 * Safe to call multiple times - will only connect once.
 */
export function initSseServerEvents(): void {
  connect();
}

/**
 * Cleanup SSE connection.
 * Called on app unmount or when switching to Tauri mode.
 */
export function cleanupSseServerEvents(): void {
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
  reconnectAttempt = 0;
}
