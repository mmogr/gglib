/**
 * Tauri Event Adapter for Server Lifecycle Events
 *
 * Listens to Tauri events and normalizes them to the shared ServerEvent type,
 * then ingests them into the server registry.
 *
 * Events listened to:
 * - server:snapshot - Snapshot of running servers
 * - server:started  - Server started and ready
 * - server:stopped  - Server stopped cleanly
 * - server:error    - Server encountered an error
 * - server:health_changed - Health status changed
 */

import { ingestServerEvent, type ServerEvent } from './serverRegistry';

type UnlistenFn = () => void;

let unlisteners: UnlistenFn[] = [];
let initialized = false;

/**
 * Normalize a Tauri event payload to our ServerEvent type.
 * 
 * Tauri events come in as the wrapped ServerEvent from Rust, which uses
 * the tagged enum format. We need to extract the inner data.
 */
function normalizeEvent(eventType: string, payload: unknown): ServerEvent | null {
  // The Rust backend emits AppEvent payloads.
  
  if (typeof payload !== 'object' || payload === null) {
    console.warn('[serverEvents.tauri] Invalid payload:', payload);
    return null;
  }

  const data = payload as Record<string, unknown>;

  // Handle snapshot event (canonical: AppEvent::ServerSnapshot)
  if (eventType === 'server:snapshot') {
    const servers = data.servers;
    if (!Array.isArray(servers)) {
      console.warn('[serverEvents.tauri] Snapshot missing servers array');
      return null;
    }

    return {
      type: 'snapshot',
      servers: servers.map((s: Record<string, unknown>) => {
        const modelId = String(s.modelId ?? s.model_id ?? '');
        const port = typeof s.port === 'number' ? s.port : undefined;

        // Snapshot entries include startedAt (seconds) on the Rust side; convert to ms.
        const startedAtSeconds =
          typeof s.startedAt === 'number'
            ? s.startedAt
            : (typeof s.started_at === 'number' ? s.started_at : undefined);
        const updatedAt = startedAtSeconds ? startedAtSeconds * 1000 : Date.now();

        // Snapshot only lists running servers.
        return { modelId, status: 'running' as const, port, updatedAt };
      }),
    };
  }

  // Canonical AppEvent payloads are flat objects (modelId, port, etc).
  const info = data;
  const modelId = String(info.modelId ?? info.model_id ?? '');
  const port = typeof info.port === 'number' ? info.port : undefined;
  // New AppEvent server lifecycle events do not currently include a timestamp.
  // Use local time for ordering; health_changed events include a backend timestamp.
  const updatedAt = typeof info.updatedAt === 'number'
    ? info.updatedAt
    : (typeof info.updated_at === 'number' ? info.updated_at : Date.now());

  switch (eventType) {
    // Canonical names (AppEvent::event_name)
    case 'server:started':
      return { type: 'running', modelId, port, updatedAt };
    case 'server:stopped':
      return { type: 'stopped', modelId, port, updatedAt };
    case 'server:error':
      return modelId ? { type: 'crashed', modelId, port, updatedAt } : null;
    case 'server:health_changed': {
      // Health changed events have a nested status object
      const status = info.status as Record<string, unknown> | undefined;
      const detail = typeof info.detail === 'string' ? info.detail : undefined;
      if (!status || typeof status.status !== 'string') {
        console.warn('[serverEvents.tauri] Health event missing status:', info);
        return null;
      }
      return {
        type: 'server_health_changed',
        modelId,
        status: status as import('../types').ServerHealthStatus,
        detail,
        updatedAt,
      };
    }
    default:
      console.warn('[serverEvents.tauri] Unknown event type:', eventType);
      return null;
  }
}

/**
 * Initialize Tauri event listeners for server lifecycle events.
 * Safe to call multiple times - will only initialize once.
 */
export async function initTauriServerEvents(): Promise<void> {
  if (initialized) {
    return;
  }

  try {
    const { listen } = await import('@tauri-apps/api/event');

    const eventTypes = [
      'server:snapshot',
      'server:started',
      'server:stopped',
      'server:error',
      'server:health_changed',
    ];

    for (const eventType of eventTypes) {
      const unlisten = await listen(eventType, (event) => {
        const normalized = normalizeEvent(eventType, event.payload);
        if (normalized) {
          ingestServerEvent(normalized);
        }
      });
      unlisteners.push(unlisten);
    }

    initialized = true;
    console.debug('[serverEvents.tauri] Initialized server event listeners');
  } catch (error) {
    console.error('[serverEvents.tauri] Failed to initialize:', error);
  }
}

/**
 * Cleanup Tauri event listeners.
 * Called on app unmount or when switching to web mode.
 */
export function cleanupTauriServerEvents(): void {
  for (const unlisten of unlisteners) {
    unlisten();
  }
  unlisteners = [];
  initialized = false;
}
