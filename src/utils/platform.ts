/**
 * Platform detection utilities.
 * 
 * This is in a separate file to avoid circular dependencies,
 * as multiple service modules need this check.
 */

/**
 * Check if we're running in Tauri (desktop app) or Web UI.
 * Supports both Tauri v1 (__TAURI__) and v2 (__TAURI_INTERNALS__).
 */
export const isTauriApp =
  typeof (window as any).__TAURI_INTERNALS__ !== 'undefined' ||
  typeof (window as any).__TAURI__ !== 'undefined';
