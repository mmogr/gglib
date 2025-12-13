/**
 * Tauri Event Adapter for Server Lifecycle Events
 *
 * Listens to Tauri events and normalizes them to the shared ServerEvent type,
 * then ingests them into the server registry.
 *
 * Events listened to:
 * - server:snapshot - Initial state of all running servers
 * - server:running - Server started and ready
 * - server:stopping - Server stop initiated
 * - server:stopped - Server stopped cleanly
 * - server:crashed - Server exited unexpectedly
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
  // The Rust ServerEvent is serialized with serde's tagged enum format
  // e.g., { type: "running", modelId: "1", port: 9000, updatedAt: 1234567890 }
  
  if (typeof payload !== 'object' || payload === null) {
    console.warn('[serverEvents.tauri] Invalid payload:', payload);
    return null;
  }

  const data = payload as Record<string, unknown>;

  // Handle snapshot event
  if (eventType === 'server:snapshot' && data.type === 'snapshot') {
    const servers = data.servers;
    if (!Array.isArray(servers)) {
      console.warn('[serverEvents.tauri] Snapshot missing servers array');
      return null;
    }
    return {
      type: 'snapshot',
      servers: servers.map((s: Record<string, unknown>) => ({
        modelId: String(s.modelId ?? s.model_id ?? ''),
        status: (s.status as 'running') ?? 'running',
        port: typeof s.port === 'number' ? s.port : undefined,
        updatedAt: typeof s.updatedAt === 'number' ? s.updatedAt : (typeof s.updated_at === 'number' ? s.updated_at : Date.now()),
      })),
    };
  }

  // Handle individual lifecycle events
  // The event payload contains the full ServerEvent including inner ServerStateInfo
  const innerData = (data as { Running?: unknown; Stopping?: unknown; Stopped?: unknown; Crashed?: unknown });
  const stateInfo = innerData.Running ?? innerData.Stopping ?? innerData.Stopped ?? innerData.Crashed ?? data;
  
  if (typeof stateInfo !== 'object' || stateInfo === null) {
    console.warn('[serverEvents.tauri] Could not extract state info from:', data);
    return null;
  }

  const info = stateInfo as Record<string, unknown>;
  const modelId = String(info.modelId ?? info.model_id ?? '');
  const port = typeof info.port === 'number' ? info.port : undefined;
  const updatedAt = typeof info.updatedAt === 'number' ? info.updatedAt : (typeof info.updated_at === 'number' ? info.updated_at : Date.now());

  switch (eventType) {
    case 'server:running':
      return { type: 'running', modelId, port, updatedAt };
    case 'server:stopping':
      return { type: 'stopping', modelId, port, updatedAt };
    case 'server:stopped':
      return { type: 'stopped', modelId, port, updatedAt };
    case 'server:crashed':
      return { type: 'crashed', modelId, port, updatedAt };
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
      'server:running',
      'server:stopping',
      'server:stopped',
      'server:crashed',
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
