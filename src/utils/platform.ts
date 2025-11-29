/**
 * Platform detection utilities.
 * 
 * This is in a separate file to avoid circular dependencies,
 * as both services/tauri.ts and utils/apiBase.ts need this check.
 */

/**
 * Check if we're running in Tauri (desktop app) or Web UI.
 * Supports both Tauri v1 (__TAURI__) and v2 (__TAURI_INTERNALS__).
 */
export const isTauriApp =
  typeof (window as any).__TAURI_INTERNALS__ !== 'undefined' ||
  typeof (window as any).__TAURI__ !== 'undefined';
