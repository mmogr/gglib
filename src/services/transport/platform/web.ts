/**
 * Web platform transport implementation.
 * Provides stub implementations for OS-specific operations that don't work in web.
 */

import { TransportError } from '../errors';

/**
 * Check llama.cpp binary installation status.
 * Not supported in web environment.
 */
export async function checkLlamaStatus(): Promise<{ installed: boolean; version?: string }> {
  throw new TransportError('NOT_SUPPORTED', 'llama.cpp installation check not supported in web');
}

/**
 * Install llama.cpp binary.
 * Not supported in web environment.
 */
export async function installLlama(): Promise<void> {
  throw new TransportError('NOT_SUPPORTED', 'llama.cpp installation not supported in web');
}

/**
 * Open URL in system browser.
 * Uses window.open as fallback.
 */
export async function openUrl(url: string): Promise<void> {
  window.open(url, '_blank');
}

/**
 * Sync menu state.
 * Not supported in web environment.
 */
export async function syncMenu(_state: unknown): Promise<void> {
  // No-op in web (no native menus)
}
