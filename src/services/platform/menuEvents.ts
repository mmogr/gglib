/**
 * Desktop menu event utilities
 * TRANSPORT_EXCEPTION: Uses Tauri events for native menu integration.
 * UI components should import from 'services/platform' rather than checking for Tauri directly.
 */

import { isDesktop } from './detect';

/**
 * Menu event names: the `names` block of `src-tauri/src/app/events.rs`,
 * held to it by `tests/ts/contracts/menuEvents.test.ts`. A place is one
 * NAVIGATE carrying a `MENU_PLACES` id; an action on the inspected model is
 * an event of its own. The endpoint's Turn On / Off is done in Rust and sends
 * nothing; its Copy sends the URL to put on the clipboard.
 */
export const MENU_EVENTS = {
  NAVIGATE: 'menu:navigate',
  COPY_TO_CLIPBOARD: 'menu:copy-to-clipboard',
  HIDE_INSPECTOR: 'menu:hide-inspector',
  LOAD_MODEL: 'menu:load-model',
  LOAD_WITH_OPTIONS: 'menu:load-with-options',
  UNLOAD_MODEL: 'menu:unload-model',
  REMOVE_MODEL: 'menu:remove-model',
} as const;

/** The places NAVIGATE names: the `places` block of `events.rs`, held to it by the same test. */
export const MENU_PLACES = [
  'library',
  'endpoint',
  'devices',
  'chat',
  'new-chat',
  'settings',
  'diagnostics',
  'get-models',
  'add-file',
  'connect-client',
] as const;
export type MenuPlace = (typeof MENU_PLACES)[number];

type MenuEvent = (typeof MENU_EVENTS)[keyof typeof MENU_EVENTS];

/** One handler per event; NAVIGATE's and COPY_TO_CLIPBOARD's use their payload, a string. */
export type MenuEventHandlers = Partial<Record<MenuEvent, (payload: string) => void>>;

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
      const unlisten = await listen<string>(event, (e) => handler(e.payload));
      unlisteners.push(unlisten);
    }
  }

  return () => {
    unlisteners.forEach(unlisten => unlisten());
  };
}
