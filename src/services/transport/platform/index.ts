/**
 * Platform transport factory.
 * Detects environment and returns appropriate platform operations.
 */

import * as tauri from './tauri';
import * as web from './web';

/**
 * Detect if running in Tauri environment.
 */
function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Create platform transport based on environment.
 * Returns plain object with OS-specific methods.
 */
export function createPlatform() {
  if (isTauri()) {
    return tauri;
  } else {
    return web;
  }
}
