/**
 * The native menus, wired to the app from outside React.
 *
 * A place is one `menu:navigate` event carrying a place id (`MENU_PLACES`):
 * ⌘1–⌘4 the library, the endpoint, the devices and the latest chat (else an
 * empty one), Settings… (the app menu's and the tray's), Diagnostics, New
 * Chat, Get Models…, Add a GGUF File… and Connect a Client…, each turned
 * into its address here and gone to through the router. The Model menu's
 * Load, Load with Options…, Unload and Remove from Library… act on the model
 * the inspector shows (`menuModelId`), with the inspector's own words; Hide
 * Inspector closes it the way its × does. The menu is told what is selected
 * from the address on every move, so it greys out what has nothing to act
 * on. The endpoint's Turn On / Off is the menu's own, in Rust; its Copy
 * arrives here only for the clipboard.
 *
 * Off a Mac there is no menu bar, so Ctrl+N is a window key that starts a
 * chat; on the desktop app on a Mac, File › New Chat ⌘N is the native item.
 *
 * The event names and place ids are pinned to the Rust that sends them by
 * `tests/ts/contracts/menuEvents.test.ts`.
 *
 * @module app/menuBridge
 */

import type { ToastType } from '../components/Toast';
import { removeModel } from '../services/modelEvents';
import { getModelLibrary } from '../services/modelRegistry';
import { chatsHere } from '../services/chatRegistry';
import {
  appLogger,
  isDesktop,
  listenToMenuEvents,
  MENU_EVENTS,
  MENU_PLACES,
  setSelectedModel,
  syncMenuStateSilent,
  type MenuPlace,
} from '../services/platform';
import { safeStopServer } from '../services/server/safeActions';
import { isServerRunning } from '../services/serverRegistry';
import { getTransport } from '../services/transport';
import { formatError } from '../utils/errors';
import { confirm } from './confirm';
import { inspectorClosed, inspectorSubject, menuModelId, paths } from './paths';
import { appRouter } from './router';

export type Notify = (message: string, type?: ToastType) => void;

/** The model the menu acts on: the inspector's. */
function selectedModelId(): number | null {
  return menuModelId(appRouter().state.location);
}

function nameOf(id: number): string {
  return getModelLibrary().models.find((m) => m.id === id)?.name ?? 'this model';
}

function navigate(to: string): void {
  void appRouter().navigate(to);
}

/** The latest chat kept here, else an empty one (which picks its model). */
async function latestChat(): Promise<string> {
  try {
    const [latest] = await chatsHere();
    return latest ? paths.chat(latest.id) : paths.newChat();
  } catch {
    return paths.newChat();
  }
}

/** Where each place the menu names is. */
const PLACES: Readonly<Record<MenuPlace, () => string | Promise<string>>> = {
  library: () => paths.library(),
  endpoint: () => paths.endpoint(),
  devices: () => paths.devices(),
  chat: latestChat,
  'new-chat': () => paths.newChat(),
  settings: () => paths.settings(),
  // On Live, where the sidebar's Diagnostics row goes too.
  diagnostics: () => paths.diagnostics(),
  'get-models': () => paths.libraryAdd('browse'),
  'add-file': () => paths.libraryAdd('add'),
  'connect-client': () => paths.endpointConnect(),
};

function isPlace(place: string): place is MenuPlace {
  return (MENU_PLACES as readonly string[]).includes(place);
}

async function goTo(place: string): Promise<void> {
  if (!isPlace(place)) {
    appLogger.warn('component.app', 'The menu named a place the app does not know', { place });
    return;
  }
  navigate(await PLACES[place]());
}

/** Load: one click, the model's id and nothing else, as the inspector's Load. */
async function loadSelected(notify: Notify): Promise<void> {
  const selected = selectedModelId();
  if (selected === null || isServerRunning(selected)) return;
  try {
    await getTransport().serveModel({ id: selected });
  } catch (error) {
    notify(`Didn't load: ${formatError(error)}`, 'error');
    return;
  }
  syncMenuStateSilent();
}

/** Load with Options…: the inspector's sheet, beside whichever place shows it. */
function loadWithOptions(): void {
  const selected = selectedModelId();
  if (selected === null) return;
  const onEndpoint = appRouter().state.location.pathname === paths.endpoint();
  navigate(onEndpoint ? paths.endpointServe(selected) : paths.serve(selected));
}

async function unloadSelected(): Promise<void> {
  const selected = selectedModelId();
  if (selected === null || !isServerRunning(selected)) return;
  await safeStopServer(selected);
  syncMenuStateSilent();
}

async function removeSelected(notify: Notify): Promise<void> {
  const selected = selectedModelId();
  if (selected === null) return;
  const name = nameOf(selected);
  const confirmed = await confirm({
    title: `Remove ${name} from the library?`,
    description: 'The file stays on disk.',
    confirmLabel: 'Remove',
    variant: 'danger',
  });
  if (!confirmed) return;
  try {
    await removeModel(selected);
  } catch (error) {
    // A served model is refused: said here, since nothing else sees it.
    notify(`Could not remove ${name}: ${formatError(error)}`, 'error');
    return;
  }
  if (selectedModelId() === selected) navigate(inspectorClosed(appRouter().state.location));
  syncMenuStateSilent();
}

function hideInspector(): void {
  const location = appRouter().state.location;
  if (inspectorSubject(location) !== null) navigate(inspectorClosed(location));
}

/** The handlers, one per event the menus send. */
export function menuHandlers(notify: Notify) {
  return {
    [MENU_EVENTS.NAVIGATE]: (place: string) => void goTo(place),
    [MENU_EVENTS.HIDE_INSPECTOR]: hideInspector,
    [MENU_EVENTS.LOAD_MODEL]: () => void loadSelected(notify),
    [MENU_EVENTS.LOAD_WITH_OPTIONS]: loadWithOptions,
    [MENU_EVENTS.UNLOAD_MODEL]: () => void unloadSelected(),
    [MENU_EVENTS.REMOVE_MODEL]: () => void removeSelected(notify),
    [MENU_EVENTS.COPY_TO_CLIPBOARD]: (payload: string) => {
      if (payload) {
        navigator.clipboard.writeText(payload).catch((error) =>
          appLogger.error('component.app', 'Failed to copy to clipboard', { error }),
        );
      }
    },
  };
}

/** Whether a native menu bar holds New Chat: the desktop app on a Mac. */
function menuBarHasNewChat(): boolean {
  return isDesktop() && /Mac|iPhone|iPad/.test(navigator.platform);
}

/**
 * Listen to the menu, and keep it told what is selected, until the returned
 * function is called. `notify` is where its failures are said.
 */
export function startMenuBridge({ notify }: { notify: Notify }): () => void {
  let stopped = false;
  let stopListening: (() => void) | null = null;
  void listenToMenuEvents(menuHandlers(notify)).then((off) => {
    if (stopped) off();
    else stopListening = off;
  });

  // The menu's selected model follows the address, told only when it changes.
  let told: number | null | undefined;
  const tell = () => {
    const id = selectedModelId();
    if (id === told) return;
    told = id;
    setSelectedModel(id).catch((err) => {
      appLogger.warn('hook.models', 'Failed to sync model selection with menu', { error: err });
    });
  };
  tell();
  const unsubscribe = appRouter().subscribe(tell);

  // Where no menu bar has New Chat, its key is the window's: ⌘N in a browser
  // on a Mac, Ctrl+N elsewhere (Ctrl+N is a Mac text field's next line).
  const mac = /Mac|iPhone|iPad/.test(navigator.platform);
  const onKey = (event: KeyboardEvent) => {
    const command = mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
    if (!command || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'n') return;
    event.preventDefault();
    navigate(paths.newChat());
  };
  const keyed = !menuBarHasNewChat();
  if (keyed) window.addEventListener('keydown', onKey);

  return () => {
    stopped = true;
    stopListening?.();
    unsubscribe();
    if (keyed) window.removeEventListener('keydown', onKey);
  };
}
