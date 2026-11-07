// TRANSPORT_EXCEPTION: Desktop-only window integration
// The one window action a webview asks for: bring the main window forward.
// UI components may import from platform/, but clients/ must NOT.

import { isDesktop } from "./detect";

/**
 * Show and focus the main window (`show_main_window`, which is the tray
 * menu's Open gglib). The tray panel's "Open gglib" is its only caller.
 * A no-op on web, where there is no tray and the page is the window.
 */
export async function showMainWindow(): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('show_main_window');
  }
}
