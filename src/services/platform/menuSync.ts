// TRANSPORT_EXCEPTION: Desktop-only shell integration
// This file contains platform-specific code for native menu state synchronization.
// UI components may import from platform/, but clients/ must NOT.

import { isDesktop } from "./detect";

/**
 * Set the currently selected model ID to sync menu state.
 * This is only relevant for the Tauri desktop app.
 */
export async function setSelectedModel(modelId: number | null): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('set_selected_model', { modelId });
  }
  // No-op for web UI - menu sync is only needed for native menus
}

/**
 * Manually trigger a menu state sync.
 * Call this after actions that might affect menu state.
 */
export async function syncMenuState(): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('sync_menu_state');
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

/**
 * Update proxy state and sync menu.
 * Call this when the proxy is started or stopped.
 */
export async function setProxyState(running: boolean, port: number | null): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('set_proxy_state', { running, port });
  }
  // No-op for web UI
}
