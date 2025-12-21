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
import type { ServerHealthStatus } from '../types';

// Re-export for convenience
export type { ServerHealthStatus } from '../types';

// ============================================================================
// Types
// ============================================================================

export type ServerStatus = 'running' | 'stopping' | 'stopped' | 'crashed';

export interface ServerState {
  status: ServerStatus;
  port?: number;
  updatedAt: number;
  /** Server health status from continuous monitoring */
  health?: ServerHealthStatus;
}

export interface ServerStateInfo {
  modelId: string;
  status: ServerStatus;
  port?: number;
  updatedAt: number;
  health?: ServerHealthStatus;
}

export type ServerEvent =
  | { type: 'snapshot'; servers: ServerStateInfo[] }
  | { type: 'running'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopping'; modelId: string; port?: number; updatedAt: number }
  | { type: 'stopped'; modelId: string; port?: number; updatedAt: number }
  | { type: 'crashed'; modelId: string; port?: number; updatedAt: number }
  | { type: 'server_health_changed'; modelId: string; status: ServerHealthStatus; detail?: string; updatedAt: number };

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
            health: server.health,
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
          // Clear health on lifecycle transitions (will be updated by health monitor)
          health: evt.type === 'running' ? { status: 'healthy' } : undefined,
        });
        notifyListeners();
      }
      break;
    }

    case 'server_health_changed': {
      const existing = state.get(evt.modelId);
      // Only update health if server exists and event is newer
      if (existing && evt.updatedAt >= existing.updatedAt) {
        state.set(evt.modelId, {
          ...existing,
          health: evt.status,
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
// Predicates
// ============================================================================

/**
 * Internal predicate to check if a ServerState represents a running server.
 * Used by both hooks and service functions to centralize "what counts as running".
 */
function isRunningState(s?: ServerState): boolean {
  return s?.status === 'running';
}

/**
 * Check if a server is currently running by model ID.
 * For use in services (non-React). UI should use useIsServerRunning() hook.
 */
export function isServerRunning(modelId: string | number): boolean {
  return isRunningState(getServerState(String(modelId)));
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
  return isRunningState(state);
}

/**
 * React hook to get a server's health status.
 * Returns undefined if server is not running or health is unknown.
 */
export function useServerHealth(modelId: string | number): ServerHealthStatus | undefined {
  const state = useServerState(modelId);
  return state?.health;
}
