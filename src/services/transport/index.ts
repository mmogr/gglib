/**
 * Transport factory and singleton accessor.
 * 
 * This is the ONLY place platform detection happens.
 * All clients should use getTransport() to access the transport instance.
 */

import type { Transport } from './types';
import { TauriTransport } from './tauri';
import { HttpTransport } from './http';

// Internal singleton storage
let _transport: Transport | null = null;

/**
 * Detect if running in Tauri environment.
 */
function detectTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Get the transport singleton.
 * 
 * Creates the appropriate transport implementation on first call.
 * Subsequent calls return the same instance.
 * 
 * @returns The transport instance for the current platform
 */
export function getTransport(): Transport {
  if (_transport) {
    return _transport;
  }

  _transport = detectTauri() 
    ? new TauriTransport() 
    : new HttpTransport();

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
