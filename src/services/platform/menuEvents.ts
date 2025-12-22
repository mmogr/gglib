/**
 * Desktop menu event utilities
 * TRANSPORT_EXCEPTION: Uses Tauri events for native menu integration.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

/**
 * Menu event names - must match event names emitted by Rust backend.
 * Single source of truth for menu event types to prevent drift.
 */
export const MENU_EVENTS = {
  OPEN_SETTINGS: 'menu:open-settings',
  TOGGLE_SIDEBAR: 'menu:toggle-sidebar',
  ADD_MODEL_FILE: 'menu:add-model-file',
  SHOW_DOWNLOADS: 'menu:show-downloads',
  REFRESH_MODELS: 'menu:refresh-models',
  START_SERVER: 'menu:start-server',
  STOP_SERVER: 'menu:stop-server',
  REMOVE_MODEL: 'menu:remove-model',
  SHOW_CHAT: 'menu:show-chat',
  INSTALL_LLAMA: 'menu:install-llama',
  CHECK_LLAMA_STATUS: 'menu:check-llama-status',
  COPY_TO_CLIPBOARD: 'menu:copy-to-clipboard',
  PROXY_ERROR: 'menu:proxy-error',
  PROXY_STOPPED: 'menu:proxy-stopped',
  START_PROXY: 'menu:start-proxy',
} as const;

export type MenuEventType = typeof MENU_EVENTS[keyof typeof MENU_EVENTS];

export interface MenuEventHandlers {
  [MENU_EVENTS.OPEN_SETTINGS]?: () => void;
  [MENU_EVENTS.TOGGLE_SIDEBAR]?: () => void;
  [MENU_EVENTS.ADD_MODEL_FILE]?: () => void;
  [MENU_EVENTS.SHOW_DOWNLOADS]?: () => void;
  [MENU_EVENTS.REFRESH_MODELS]?: () => void;
  [MENU_EVENTS.START_SERVER]?: () => void;
  [MENU_EVENTS.STOP_SERVER]?: () => void;
  [MENU_EVENTS.REMOVE_MODEL]?: () => void;
  [MENU_EVENTS.SHOW_CHAT]?: () => void;
  [MENU_EVENTS.INSTALL_LLAMA]?: () => void;
  [MENU_EVENTS.CHECK_LLAMA_STATUS]?: () => void;
  [MENU_EVENTS.COPY_TO_CLIPBOARD]?: (payload: string) => void;
  [MENU_EVENTS.PROXY_ERROR]?: (payload: string) => void;
  [MENU_EVENTS.PROXY_STOPPED]?: () => void;
  [MENU_EVENTS.START_PROXY]?: () => void;
}

/**
 * Listen for native menu events (Tauri only).
 * Returns an unsubscribe function that cleans up all listeners.
 * No-op on web (returns empty function).
 */
export async function listenToMenuEvents(
  handlers: MenuEventHandlers
): Promise<() => void> {
  if (!isDesktop()) {
    return () => {};
  }

  const { listen } = await import('@tauri-apps/api/event');
  const unlisteners: (() => void)[] = [];

  for (const [event, handler] of Object.entries(handlers)) {
    if (handler) {
      const unlisten = await listen(event, (e: any) => {
        // Events with string payloads
        if (event === MENU_EVENTS.COPY_TO_CLIPBOARD || event === MENU_EVENTS.PROXY_ERROR) {
          (handler as (payload: string) => void)(e.payload);
        } else {
          (handler as () => void)();
        }
      });
      unlisteners.push(unlisten);
    }
  }

  return () => {
    unlisteners.forEach(unlisten => unlisten());
  };
}
