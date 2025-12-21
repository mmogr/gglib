/**
 * Safe Server Actions
 *
 * Centralized guard helpers for server operations. Wraps backend calls
 * with server-running checks to avoid 500s when the server is already stopped.
 *
 * UI components should use these instead of calling stopServer directly.
 */

import { isServerRunning } from '../serverRegistry';
import { stopServer } from '../clients/servers';

/**
 * Safely stop a server for a model.
 *
 * If the server is not running, this is a no-op (returns immediately).
 * If the server is running, calls stopServer and propagates any errors.
 *
 * @param modelId - The model ID to stop
 * @returns Promise that resolves when stop completes (or immediately if not running)
 */
export async function safeStopServer(modelId: number): Promise<void> {
  if (!isServerRunning(modelId)) {
    // Server already stopped — no-op
    return;
  }

  try {
    await stopServer(modelId);
  } catch (error) {
    // If we get an error indicating the server is not running, treat as success
    // This handles race conditions where server stopped between our check and the call
    const message = error instanceof Error ? error.message : String(error);
    if (message.includes('not running')) {
      return;
    }
    throw error;
  }
}
