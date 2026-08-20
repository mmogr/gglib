/**
 * Transport factory and singleton accessor.
 *
 * Composes the HTTP API client with the SSE event bus into one object and
 * memoises it. There is no transport to choose between: desktop and web both
 * reach the daemon the same way, and what platform difference remains —
 * resolving the base URL, picking a retry path — is absorbed inside
 * `api/client.ts`.
 *
 * All clients should use getTransport() to access the transport instance.
 */

import { createApiTransport } from './api';
import { createEventBus } from './events';

/**
 * What `getTransport()` hands back: the HTTP client and the event bus, spread
 * into one object. Inferred from the two factories rather than restated as an
 * interface, so it cannot drift from what they actually return.
 */
type TransportInstance = ReturnType<typeof createApiTransport> &
  ReturnType<typeof createEventBus>;

// Internal singleton storage
let _transport: TransportInstance | null = null;

/**
 * Get the transport singleton.
 * 
 * Creates the unified transport by composing the API and events modules.
 * Subsequent calls return the same instance.
 *
 * @returns The process-wide transport instance
 */
export function getTransport(): TransportInstance {
  if (_transport) {
    return _transport;
  }

  const api = createApiTransport();
  const events = createEventBus();

  const transport = {
    ...api,
    ...events,
  };
  
  _transport = transport;
  return _transport;
}


// Re-export types for convenience
export * from './types';
export * from './errors';
