/**
 * Transport factory and singleton accessor.
 * 
 * Composes API, platform, and event modules into a unified Transport object.
 * This is the ONLY place platform detection happens.
 * All clients should use getTransport() to access the transport instance.
 */

import type { Transport } from './types';
import { createApiTransport } from './api';
import { createPlatform } from './platform';
import { createEventBus } from './events';
import { checkCollisions } from './utils';

// Internal singleton storage
let _transport: Transport | null = null;

/**
 * Get the transport singleton.
 * 
 * Creates the unified transport by composing API, platform, and events modules.
 * Subsequent calls return the same instance.
 * 
 * @returns The transport instance for the current platform
 */
export function getTransport(): Transport {
  if (_transport) {
    return _transport;
  }

  // Create all transport modules
  const api = createApiTransport();
  const platform = createPlatform();
  const events = createEventBus();
  
  // Check for collisions in dev mode
  checkCollisions(api, platform, events);
  
  // Compose into unified transport with explicit interface satisfaction
  const transport = {
    ...api,
    ...platform,
    ...events,
  } satisfies Transport;
  
  _transport = transport;
  return _transport;
}

/**
 * Reset the transport singleton.
 * 
 * Primarily for testing purposes.
 * Allows injection of a mock transport.
 */
export function _resetTransport(transport?: Transport): void {
  _transport = transport ?? null;
}

// Re-export types for convenience
export type { Transport } from './types';
export * from './types';
export * from './errors';
