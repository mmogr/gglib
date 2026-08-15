/**
 * Transport factory and singleton accessor.
 *
 * Composes the HTTP API client with the SSE event bus into a unified
 * Transport object, checks the two for key collisions, and memoises the
 * result. There is no transport to choose between: desktop and web both
 * reach the daemon the same way, and what platform difference remains —
 * resolving the base URL, picking a retry path — is absorbed inside
 * `api/client.ts`.
 *
 * All clients should use getTransport() to access the transport instance.
 */

import type { Transport } from './types';
import { createApiTransport } from './api';
import { createEventBus } from './events';
import { checkCollisions } from './utils';

// Internal singleton storage
let _transport: Transport | null = null;

/**
 * Get the transport singleton.
 * 
 * Creates the unified transport by composing the API and events modules.
 * Subsequent calls return the same instance.
 *
 * @returns The process-wide transport instance
 */
export function getTransport(): Transport {
  if (_transport) {
    return _transport;
  }

  // Create all transport modules
  const api = createApiTransport();
  const events = createEventBus();
  
  // Check for collisions in dev mode
  checkCollisions(api, events);
  
  // Compose into unified transport with explicit interface satisfaction
  const transport = {
    ...api,
    ...events,
  } satisfies Transport;
  
  _transport = transport;
  return _transport;
}


// Re-export types for convenience
export type { Transport } from './types';
export * from './types';
export * from './errors';
