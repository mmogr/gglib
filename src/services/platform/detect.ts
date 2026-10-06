/**
 * Platform detection utilities
 * TRANSPORT_EXCEPTION: This module is the one place that asks whether this is the desktop app.
 * UI code should import isDesktop() from 'services/platform' rather than reading the global itself.
 */

declare global {
  interface Window {
    /** Tauri v2's bridge, put on every window of the desktop app before its scripts run. */
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

/**
 * Returns true if running in the Tauri desktop app: its main window or its
 * tray panel, and never a browser tab.
 *
 * Asked on each call, not once when the module loads, so the answer is the
 * same wherever it is asked from. Tauri v1's `__TAURI__` is not looked for:
 * the app is built on v2 without `withGlobalTauri`, so nothing sets it.
 */
export function isDesktop(): boolean {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ !== undefined;
}
