// System domain operations
// Desktop-specific utilities (menu sync, URL opening, etc.)

import { isTauriApp, tauriInvoke } from "./base";

/**
 * Open a URL in the system's default browser.
 * Falls back to window.open for web UI / dev mode.
 */
export async function openUrl(url: string): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('open_url', { url });
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

/**
 * Set the currently selected model ID to sync menu state.
 * This is only relevant for the Tauri desktop app.
 */
export async function setSelectedModel(modelId: number | null): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('set_selected_model', { modelId });
  }
  // No-op for web UI - menu sync is only needed for native menus
}

/**
 * Manually trigger a menu state sync.
 * Call this after actions that might affect menu state.
 */
export async function syncMenuState(): Promise<void> {
  if (isTauriApp) {
    await tauriInvoke('sync_menu_state');
  }
  // No-op for web UI
}

/**
 * Trigger menu state sync silently (swallowing errors).
 * Use this for fire-and-forget sync after state-changing operations.
 */
export function syncMenuStateSilent(): void {
  syncMenuState().catch(() => {
    // Silently ignore - menu sync is best-effort
  });
}
