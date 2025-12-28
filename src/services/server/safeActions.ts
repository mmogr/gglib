/**
 * Safe Server Actions
 *
 * Centralized helpers for server operations.
 *
 * Backend is the source of truth for lifecycle state; client-side state can be stale.
 * Actions here are written to be idempotent and resilient to state desync.
 *
 * UI components should use these instead of calling stopServer directly.
 */

import { stopServer } from '../clients/servers';
import { TransportError } from '../transport/errors';

/**
 * Safely stop a server for a model.
 *
 * Always attempts to stop via backend. "Already stopped" responses are treated
 * as success (idempotent stop).
 *
 * @param modelId - The model ID to stop
 * @returns Promise that resolves when stop completes
 */
export async function safeStopServer(modelId: number): Promise<void> {
  try {
    await stopServer(modelId);
  } catch (error) {
    // Backend is the source of truth. If it reports the server is already stopped,
    // treat that as success (idempotent stop).
    if (TransportError.hasCode(error, 'NOT_FOUND') || TransportError.hasCode(error, 'CONFLICT')) {
      return;
    }

    // Back-compat for older error strings.
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes('not running')) return;
    throw error;
  }
}
