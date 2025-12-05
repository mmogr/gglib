/**
 * Server State Registry
 *
 * Minimal external store for server lifecycle state. Backend events are the
 * sole source of truth for server lifecycle transitions.
 *
 * Key design decisions:
 * - Keyed by modelId (string) for frontend/JSON compatibility
 * - getServerState returns undefined for unknown models (UI treats as not running)
 * - Uses useSyncExternalStore for React integration
 * - Single ingestServerEvent function handles all event types
 * - Ordering guard via updatedAt prevents stale event overwrites
 */

import { useSyncExternalStore } from 'react';

// ============================================================================
// Types
// ============================================================================

export type ServerStatus = 'running' | 'stopping' | 'stopped' | 'crashed';

export interface ServerState {
  status: ServerStatus;
  port?: number;
  updatedAt: number;
}

export interface ServerStateInfo {
  modelId: string;
  status: ServerStatus;
  port?: number;
  updatedAt: number;
}

export type ServerEvent =
  | { type: 'snapshot'; servers: ServerStateInfo[] }
  | { type: 'running'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopping'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopped'; modelId: string; port?: number; updatedAt: number }
  | { type: 'crashed'; modelId: string; port?: number; updatedAt: number };

// ============================================================================
// Registry State
// ============================================================================

const state = new Map<string, ServerState>();
const listeners = new Set<() => void>();

// ============================================================================
// Registry API
// ============================================================================

/**
 * Get the current state of a server by model ID.
 * Returns undefined for unknown models (UI should treat as not running).
 */
export function getServerState(modelId: string): ServerState | undefined {
  return state.get(modelId);
}

/**
 * Get all server states as a snapshot.
 * Used for debugging and initial hydration checks.
 */
export function getAllServerStates(): Map<string, ServerState> {
  return new Map(state);
}

/**
 * Subscribe to state changes.
 * Returns an unsubscribe function.
 */
export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Notify all listeners of a state change.
 */
function notifyListeners(): void {
  listeners.forEach((listener) => listener());
}

/**
 * Ingest a server event and update state accordingly.
 * 
 * Ordering guard: Only applies update if evt.updatedAt >= existing.updatedAt.
 * This protects against out-of-order event delivery (e.g., snapshot arriving
 * after individual events).
 */
export function ingestServerEvent(evt: ServerEvent): void {
  switch (evt.type) {
    case 'snapshot': {
      // Snapshot contains only running servers
      // Clear existing state and replace with snapshot
      // Note: We don't clear stopped/crashed servers as snapshot only shows running
      for (const server of evt.servers) {
        const existing = state.get(server.modelId);
        if (!existing || server.updatedAt >= existing.updatedAt) {
          state.set(server.modelId, {
            status: server.status,
            port: server.port,
            updatedAt: server.updatedAt,
          });
        }
      }
      notifyListeners();
      break;
    }

    case 'running':
    case 'stopping':
    case 'stopped':
    case 'crashed': {
      const existing = state.get(evt.modelId);
      if (!existing || evt.updatedAt >= existing.updatedAt) {
        state.set(evt.modelId, {
          status: evt.type,
          port: evt.port,
          updatedAt: evt.updatedAt,
        });
        notifyListeners();
      }
      break;
    }
  }
}

/**
 * Clear all server state. Used for testing and cleanup.
 */
export function clearAllServerState(): void {
  state.clear();
  notifyListeners();
}

// ============================================================================
// React Hook
// ============================================================================

/**
 * React hook to subscribe to a specific server's state.
 * 
 * Returns undefined for unknown models (UI should treat as not running).
 * Automatically re-renders when the server's state changes.
 * Polling resumes automatically when status changes to 'running' via a new
 * server:running event.
 */
export function useServerState(modelId: string | number): ServerState | undefined {
  const modelIdStr = String(modelId);
  
  return useSyncExternalStore(
    subscribe,
    () => getServerState(modelIdStr),
    () => getServerState(modelIdStr) // Server-side render fallback (same as client)
  );
}

/**
 * React hook to check if a server is currently running.
 * Convenience wrapper around useServerState.
 */
export function useIsServerRunning(modelId: string | number): boolean {
  const state = useServerState(modelId);
  return state?.status === 'running';
}
