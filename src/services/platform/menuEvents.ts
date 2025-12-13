/**
 * Desktop menu event utilities
 * TRANSPORT_EXCEPTION: Uses Tauri events for native menu integration.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

export type MenuEventType = 
  | 'menu:open-settings'
  | 'menu:toggle-sidebar'
  | 'menu:add-model-file'
  | 'menu:refresh-models'
  | 'menu:start-server'
  | 'menu:stop-server'
  | 'menu:remove-model'
  | 'menu:install-llama'
  | 'menu:check-llama-status'
  | 'menu:copy-to-clipboard'
  | 'menu:proxy-stopped'
  | 'menu:start-proxy';

export interface MenuEventHandlers {
  'menu:open-settings'?: () => void;
  'menu:toggle-sidebar'?: () => void;
  'menu:add-model-file'?: () => void;
  'menu:refresh-models'?: () => void;
  'menu:start-server'?: () => void;
  'menu:stop-server'?: () => void;
  'menu:remove-model'?: () => void;
  'menu:install-llama'?: () => void;
  'menu:check-llama-status'?: () => void;
  'menu:copy-to-clipboard'?: (payload: string) => void;
  'menu:proxy-stopped'?: () => void;
  'menu:start-proxy'?: () => void;
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
        if (event === 'menu:copy-to-clipboard') {
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
