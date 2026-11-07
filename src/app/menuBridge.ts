/**
 * The native menu, wired to the app from outside React.
 *
 * The menu's items name places and actions: Settings, Add Model, Downloads,
 * Chat, Refresh, Start, Stop and Remove for the selected model. Navigation
 * goes through the router; a model action reads the model the address names
 * (`menuModelId`: the inspector's) and calls the
 * registries, the transport and the confirm store. The menu is told what is selected from the address too,
 * on every move, so its Start, Stop and Remove act on the model the page
 * shows. Nothing here needs a component to be mounted, which is what
 * retires the callback bridge the library page used to fill.
 *
 * The event names are pinned to the Rust that sends them by
 * `tests/ts/contracts/menuEvents.test.ts`.
 *
 * @module app/menuBridge
 */

import type { ToastType } from '../components/Toast';
import { refreshModels, removeModel } from '../services/modelEvents';
import { getModelLibrary } from '../services/modelRegistry';
import { appLogger, listenToMenuEvents, MENU_EVENTS, setSelectedModel, syncMenuStateSilent } from '../services/platform';
import { safeStopServer } from '../services/server/safeActions';
import { isServerRunning, runningModelIds } from '../services/serverRegistry';
import { getTransport } from '../services/transport';
import { formatError } from '../utils/errors';
import { confirm } from './confirm';
import { chatPathOn } from './openChat';
import { menuModelId, paths } from './paths';
import { appRouter } from './router';

export type Notify = (message: string, type?: ToastType) => void;

/** The model the menu acts on: the inspector's. */
function selectedModelId(): number | null {
  return menuModelId(appRouter().state.location);
}

function navigate(to: string): void {
  void appRouter().navigate(to);
}

/** Chat: on the selected model's server, else the first running, else nothing. */
async function showChat(notify: Notify): Promise<void> {
  const selected = selectedModelId();
  const target = selected !== null && isServerRunning(selected) ? selected : runningModelIds()[0];
  if (target === undefined) {
    notify('No servers are currently running. Start a server first to use chat.', 'warning');
    return;
  }
  navigate(await chatPathOn(target));
}

async function stopSelected(): Promise<void> {
  const selected = selectedModelId();
  if (selected === null || !isServerRunning(selected)) return;
  await safeStopServer(selected);
  syncMenuStateSilent();
}

async function removeSelected(notify: Notify): Promise<void> {
  const selected = selectedModelId();
  if (selected === null) return;
  const name = getModelLibrary().models.find((m) => m.id === selected)?.name ?? 'this model';
  const confirmed = await confirm({
    title: `Remove "${name}"?`,
    description: 'This will not delete the model file from disk.',
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
  if (selectedModelId() === selected) navigate(paths.library());
  syncMenuStateSilent();
}

/** The handlers, one per event the menu sends. */
export function menuHandlers(notify: Notify) {
  return {
    [MENU_EVENTS.OPEN_SETTINGS]: () => navigate(paths.settings()),
    [MENU_EVENTS.ADD_MODEL_FILE]: () => navigate(paths.libraryAdd('add')),
    [MENU_EVENTS.SHOW_DOWNLOADS]: () => navigate(paths.libraryAdd('browse')),
    [MENU_EVENTS.SHOW_CHAT]: () => void showChat(notify),
    [MENU_EVENTS.REFRESH_MODELS]: () => void refreshModels(),
    [MENU_EVENTS.START_SERVER]: () => {
      const selected = selectedModelId();
      if (selected !== null) navigate(paths.serve(selected));
    },
    [MENU_EVENTS.STOP_SERVER]: () => void stopSelected(),
    [MENU_EVENTS.REMOVE_MODEL]: () => void removeSelected(notify),
    [MENU_EVENTS.COPY_TO_CLIPBOARD]: (payload: string) => {
      if (payload) {
        navigator.clipboard.writeText(payload).catch((error) =>
          appLogger.error('component.app', 'Failed to copy to clipboard', { error }),
        );
      }
    },
    [MENU_EVENTS.PROXY_STOPPED]: async () => {
      try {
        await getTransport().stopProxy();
        notify('Proxy stopped', 'success');
      } catch (error) {
        notify('Failed to stop proxy', 'error');
        appLogger.error('component.app', 'Failed to stop proxy', { error });
      }
    },
    [MENU_EVENTS.START_PROXY]: async () => {
      try {
        const status = await getTransport().startProxy();
        notify(`Proxy started on port ${status.port}`, 'success');
      } catch (error) {
        notify('Failed to start proxy', 'error');
        appLogger.error('component.app', 'Failed to start proxy', { error });
      }
    },
  };
}

/**
 * Listen to the menu, and keep it told what is selected, until the returned
 * function is called. `notify` is where its proxy messages go.
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

  // ⌘N on a Mac, Ctrl+N elsewhere, starts a chat from anywhere (Ctrl+N is a
  // Mac text field's next line). A window key, not a menu item: the native
  // menu has no New Chat to send.
  const onKey = (event: KeyboardEvent) => {
    const command = /Mac|iPhone|iPad/.test(navigator.platform) ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
    if (!command || event.shiftKey || event.altKey || event.key.toLowerCase() !== 'n') return;
    event.preventDefault();
    navigate(paths.newChat());
  };
  window.addEventListener('keydown', onKey);

  return () => {
    stopped = true;
    stopListening?.();
    unsubscribe();
    window.removeEventListener('keydown', onKey);
  };
}
