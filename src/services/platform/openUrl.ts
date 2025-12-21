// TRANSPORT_EXCEPTION: Desktop-only shell integration
// This file contains platform-specific code that cannot be abstracted through the transport layer.
// UI components may import from platform/, but clients/ must NOT.

import { isDesktop } from "./detect";

/**
 * Open a URL in the system's default browser.
 * Falls back to window.open for web UI / dev mode.
 */
export async function openUrl(url: string): Promise<void> {
  if (isDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke('open_url', { url });
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}
