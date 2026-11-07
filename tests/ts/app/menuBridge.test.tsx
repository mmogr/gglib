/**
 * The native menus, acting from outside React: each place a `menu:navigate`
 * names and each model action's effect on the address and the registries,
 * New Chat's key off a Mac, and the menu told what is selected.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { guiModel } from '../fixtures/model';

type Handlers = Record<string, (payload?: string) => void | Promise<void>>;
const menu = vi.hoisted(() => ({
  handlers: null as Handlers | null,
  desktop: true,
  setSelectedModel: vi.fn(async (_id: number | null) => {}),
  syncMenuStateSilent: vi.fn(),
}));
const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => [guiModel({ id: 7, name: 'qwen3-8b' }), guiModel({ id: 9, name: 'gemma-3-12b' })]),
  removeModel: vi.fn(async (_id: number) => {}),
  stopServer: vi.fn(async (_id: number) => {}),
  serveModel: vi.fn(async (_config: { id: number }) => ({ port: 9007 })),
  getSettings: vi.fn(async () => ({})),
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
}));

vi.mock('../../../src/services/platform', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/platform')>()),
  isDesktop: () => menu.desktop,
  listenToMenuEvents: vi.fn(async (handlers: Handlers) => {
    menu.handlers = handlers;
    return () => {
      menu.handlers = null;
    };
  }),
  setSelectedModel: menu.setSelectedModel,
  syncMenuStateSilent: menu.syncMenuStateSilent,
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
// The places are not under test; each names itself.
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/places/endpoint/EndpointPlace', () => ({ default: () => <main aria-label="Endpoint" /> }));
vi.mock('../../../src/places/devices/DevicesPlace', () => ({ default: () => <main aria-label="Devices" /> }));
vi.mock('../../../src/places/settings/SettingsPlace', () => ({ default: () => <main aria-label="Settings" /> }));
vi.mock('../../../src/app/BenchmarkRoute', () => ({ BenchmarkRoute: () => <main aria-label="Benchmarks" /> }));
vi.mock('../../../src/app/ChatRoute', () => ({ ChatRoute: () => <main aria-label="Chat" /> }));

import { startMenuBridge } from '../../../src/app/menuBridge';
import { MENU_EVENTS } from '../../../src/services/platform/menuEvents';
import { resetChats } from '../../../src/services/chatRegistry';
import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { renderApp } from '../places/harness';

const notify = vi.fn();

/** The bridge started over the app at `path`, with its handlers in hand. */
async function bridged(path: string) {
  const app = renderApp(path);
  initModelEvents();
  const stop = startMenuBridge({ notify });
  await waitFor(() => expect(menu.handlers).not.toBeNull());
  const fire = (event: string, payload?: string) => act(async () => {
    await menu.handlers?.[event]?.(payload);
  });
  const go = (place: string) => fire(MENU_EVENTS.NAVIGATE, place);
  return { ...app, stop, fire, go };
}

function serve(modelId: number, type: 'running' | 'stopped' = 'running') {
  act(() => ingestServerEvent({ type, modelId: String(modelId), port: 9000 + modelId, updatedAt: Date.now(), modelName: `m${modelId}` }));
}

const press = (init: KeyboardEventInit) =>
  act(() => {
    window.dispatchEvent(new KeyboardEvent('keydown', init));
  });

const conversation = (id: number, updated: string) => ({
  id,
  title: `chat ${id}`,
  model_id: 7,
  system_prompt: null,
  created_at: updated,
  updated_at: updated,
});

describe('the menu bridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    menu.handlers = null;
    menu.desktop = true;
    transport.listConversations.mockResolvedValue([]);
    resetChats();
  });
  afterEach(() => {
    cleanupModelEvents();
    for (const id of [7, 8, 9]) serve(id, 'stopped');
    vi.restoreAllMocks();
  });

  it('tells the menu the selected model on every move, once per change', async () => {
    const { at, router, stop } = await bridged('/models/7');
    expect(menu.setSelectedModel).toHaveBeenLastCalledWith(7);
    expect(menu.setSelectedModel).toHaveBeenCalledTimes(1);

    await act(() => router.navigate('/models/7?serve=1'));
    expect(menu.setSelectedModel).toHaveBeenCalledTimes(1);
    await act(() => router.navigate('/library'));
    expect(at()).toBe('/library');
    expect(menu.setSelectedModel).toHaveBeenLastCalledWith(null);
    expect(menu.setSelectedModel).toHaveBeenCalledTimes(2);

    stop();
    await act(() => router.navigate('/models/9'));
    expect(menu.setSelectedModel).toHaveBeenCalledTimes(2);
    expect(menu.handlers).toBeNull();
  });

  it('⌘1–⌘3 go to the library, the endpoint and the devices', async () => {
    const { at, go, stop } = await bridged('/models/7');
    await go('library');
    expect(at()).toBe('/library');
    await go('endpoint');
    expect(at()).toBe('/endpoint');
    await go('devices');
    expect(at()).toBe('/devices');
    stop();
  });

  it('⌘4 goes to the latest chat, else an empty one', async () => {
    const { at, router, go, stop } = await bridged('/library');
    await go('chat');
    await waitFor(() => expect(at()).toBe('/chat/new'));

    transport.listConversations.mockResolvedValue([conversation(6, '2026-11-02 09:00:00'), conversation(4, '2026-11-01 09:00:00')]);
    resetChats();
    await act(() => router.navigate('/library'));
    await go('chat');
    await waitFor(() => expect(at()).toBe('/chat/6'));
    stop();
  });

  it('Settings… opens Settings › General, as a place of its own; Diagnostics and New Chat go to theirs', async () => {
    const { at, router, go, stop } = await bridged('/models/7');
    await go('settings');
    expect(at()).toBe('/settings/general');
    // A place of its own: Back returns to the one it was opened from.
    expect(router.state.historyAction).toBe('PUSH');
    await go('diagnostics');
    expect(at()).toBe('/diagnostics/benchmarks');
    await go('new-chat');
    expect(at()).toBe('/chat/new');
    stop();
  });

  it('Get Models… finds on Hugging Face, Add a GGUF File… adds one, Connect a Client… opens the endpoint at its block', async () => {
    const { at, go, stop } = await bridged('/library');
    await go('get-models');
    expect(at()).toBe('/library?add=browse');
    await go('add-file');
    expect(at()).toBe('/library?add=file');
    await go('connect-client');
    expect(at()).toBe('/endpoint?connect=1');
    stop();
  });

  it('a place the app does not know goes nowhere', async () => {
    const { at, go, stop } = await bridged('/models/7');
    await go('the-moon');
    expect(at()).toBe('/models/7');
    stop();
  });

  it('off a Mac, Ctrl+N starts a chat from anywhere and ⌘N does not, until the bridge stops', async () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Linux x86_64');
    const { at, router, stop } = await bridged('/models/7');
    press({ key: 'n', metaKey: true });
    expect(at()).toBe('/models/7');
    press({ key: 'N', ctrlKey: true, shiftKey: true });
    expect(at()).toBe('/models/7');
    press({ key: 'n', ctrlKey: true });
    expect(at()).toBe('/chat/new');

    stop();
    await act(() => router.navigate('/library'));
    press({ key: 'n', ctrlKey: true });
    expect(at()).toBe('/library');
  });

  it('in the desktop app on a Mac, ⌘N is the menu bar’s New Chat, not a window key', async () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('MacIntel');
    const { at, go, stop } = await bridged('/models/7');
    press({ key: 'n', metaKey: true });
    expect(at()).toBe('/models/7');
    press({ key: 'n', ctrlKey: true });
    expect(at()).toBe('/models/7');

    await go('new-chat');
    expect(at()).toBe('/chat/new');
    stop();
  });

  it('in a browser on a Mac, with no menu bar, ⌘N is the window key', async () => {
    menu.desktop = false;
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('MacIntel');
    const { at } = renderApp('/models/7');
    const stop = startMenuBridge({ notify });
    press({ key: 'n', ctrlKey: true });
    expect(at()).toBe('/models/7');
    press({ key: 'n', metaKey: true });
    expect(at()).toBe('/chat/new');
    stop();
  });

  it('Load loads the inspected model with nothing but its id, and does nothing with none inspected', async () => {
    const { at, router, fire, stop } = await bridged('/library');
    await fire(MENU_EVENTS.LOAD_MODEL);
    expect(transport.serveModel).not.toHaveBeenCalled();

    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.LOAD_MODEL);
    expect(transport.serveModel).toHaveBeenCalledWith({ id: 7 });
    expect(at()).toBe('/models/7');
    expect(menu.syncMenuStateSilent).toHaveBeenCalledTimes(1);
    stop();
  });

  it('a refused Load is said in the daemon’s words, and a served model is not loaded again', async () => {
    transport.serveModel.mockRejectedValueOnce(new Error('llama-server is not installed'));
    const { fire, stop } = await bridged('/models/7');
    await waitFor(() => expect(transport.listModels).toHaveBeenCalled());
    await fire(MENU_EVENTS.LOAD_MODEL);
    expect(notify).toHaveBeenCalledWith('Could not load qwen3-8b: llama-server is not installed', 'error');
    expect(menu.syncMenuStateSilent).not.toHaveBeenCalled();

    serve(7);
    transport.serveModel.mockClear();
    await fire(MENU_EVENTS.LOAD_MODEL);
    expect(transport.serveModel).not.toHaveBeenCalled();
    stop();
  });

  it('Load with Options… opens the sheet beside the place that shows the model', async () => {
    const { at, router, fire, stop } = await bridged('/library');
    await fire(MENU_EVENTS.LOAD_WITH_OPTIONS);
    expect(at()).toBe('/library');
    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.LOAD_WITH_OPTIONS);
    expect(at()).toBe('/models/7?serve=1');
    await act(() => router.navigate('/endpoint?inspect=model:9'));
    await fire(MENU_EVENTS.LOAD_WITH_OPTIONS);
    expect(at()).toBe('/endpoint?inspect=model:9&serve=1');
    stop();
  });

  it('Unload unloads the inspected running model, and nothing with another model inspected', async () => {
    serve(7);
    const { at, router, fire, stop } = await bridged('/models/9');
    await fire(MENU_EVENTS.UNLOAD_MODEL);
    expect(transport.stopServer).not.toHaveBeenCalled();

    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.UNLOAD_MODEL);
    expect(transport.stopServer).toHaveBeenCalledWith(7);
    expect(at()).toBe('/models/7');
    expect(menu.syncMenuStateSilent).toHaveBeenCalledTimes(1);
    stop();
  });

  it("on a chat the menu has no model: a chat's address names its conversation, so Unload unloads nothing", async () => {
    serve(7);
    const { at, fire, stop } = await bridged('/chat/7');
    expect(menu.setSelectedModel).toHaveBeenLastCalledWith(null);

    await fire(MENU_EVENTS.UNLOAD_MODEL);

    expect(transport.stopServer).not.toHaveBeenCalled();
    expect(at()).toBe('/chat/7');
    stop();
  });

  it('Remove from Library… asks in the inspector’s words, removes on Remove, and closes the inspector', async () => {
    const { at, stop } = await bridged('/models/7');
    await waitFor(() => expect(transport.listModels).toHaveBeenCalled());
    const user = userEvent.setup();
    const heading = () => screen.queryByRole('heading', { name: 'Remove qwen3-8b from the library?' });

    const declined = menu.handlers?.[MENU_EVENTS.REMOVE_MODEL]?.();
    await waitFor(() => expect(heading()).toBeInTheDocument());
    expect(screen.getByText('The file stays on disk.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /cancel/i }));
    await declined;
    await waitFor(() => expect(heading()).not.toBeInTheDocument());
    expect(transport.removeModel).not.toHaveBeenCalled();
    expect(at()).toBe('/models/7');

    const removed = menu.handlers?.[MENU_EVENTS.REMOVE_MODEL]?.();
    await waitFor(() => expect(heading()).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'Remove' }));
    await removed;
    expect(transport.removeModel).toHaveBeenCalledWith(7);
    await waitFor(() => expect(at()).toBe('/library'));
    stop();
  });

  it('a refused Remove is said, and the address stays', async () => {
    transport.removeModel.mockRejectedValueOnce(new Error('the model is being served'));
    const { at, stop } = await bridged('/models/7');
    const user = userEvent.setup();
    const asked = menu.handlers?.[MENU_EVENTS.REMOVE_MODEL]?.();
    await user.click(await screen.findByRole('button', { name: 'Remove' }));
    await asked;

    expect(notify).toHaveBeenCalledWith('Could not remove qwen3-8b: the model is being served', 'error');
    expect(at()).toBe('/models/7');
    expect(menu.syncMenuStateSilent).not.toHaveBeenCalled();
    stop();
  });

  it('Hide Inspector closes it as its × does, beside the library or the endpoint, and moves nothing else', async () => {
    const { at, router, fire, stop } = await bridged('/models/7');
    await fire(MENU_EVENTS.HIDE_INSPECTOR);
    expect(at()).toBe('/library');

    await act(() => router.navigate('/endpoint?inspect=model:7'));
    await fire(MENU_EVENTS.HIDE_INSPECTOR);
    expect(at()).toBe('/endpoint');

    await act(() => router.navigate('/devices'));
    await fire(MENU_EVENTS.HIDE_INSPECTOR);
    expect(at()).toBe('/devices');
    stop();
  });

  it('Copy puts the URL Rust sends on the clipboard', async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const { fire, stop } = await bridged('/library');
    await fire(MENU_EVENTS.COPY_TO_CLIPBOARD, 'http://127.0.0.1:8080/v1');
    expect(writeText).toHaveBeenCalledWith('http://127.0.0.1:8080/v1');
    stop();
  });
});
