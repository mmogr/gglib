// TRANSPORT_EXCEPTION: Desktop-only daemon start
// With no daemon there is no HTTP API to ask, so starting one is the app's.
// UI components may import from platform/, but clients/ must NOT.

import { isDesktop } from "./detect";

/**
 * Start the gglib daemon (`start_gglib`, the tray menu's Start gglib),
 * resolving once it answers. Rejects with the app's words: one already
 * running, the port held by another program, no `gglib` to launch. The
 * service-not-running screen is its only caller, and offers it only on the
 * desktop: a browser tab has nothing to start a daemon with.
 */
export async function startGglib(): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('start_gglib');
  }
}
