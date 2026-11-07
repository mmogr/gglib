/**
 * The native menu, acting from outside React: each event's effect on the
 * address and the registries, and the menu told what is selected.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { guiModel } from '../fixtures/model';

type Handlers = Record<string, (payload?: string) => void | Promise<void>>;
const menu = vi.hoisted(() => ({
  handlers: null as Handlers | null,
  setSelectedModel: vi.fn(async (_id: number | null) => {}),
  syncMenuStateSilent: vi.fn(),
}));
const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => [guiModel({ id: 7, name: 'qwen3-8b' }), guiModel({ id: 9, name: 'gemma-3-12b' })]),
  removeModel: vi.fn(async (_id: number) => {}),
  stopServer: vi.fn(async (_id: number) => {}),
  startProxy: vi.fn(async () => ({ running: true, port: 8080 })),
  stopProxy: vi.fn(async () => {}),
  getSettings: vi.fn(async () => ({})),
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
}));

vi.mock('../../../src/services/platform', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/platform')>()),
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
  return { ...app, stop, fire };
}

function serve(modelId: number, type: 'running' | 'stopped' = 'running') {
  act(() => ingestServerEvent({ type, modelId: String(modelId), port: 9000 + modelId, updatedAt: Date.now(), modelName: `m${modelId}` }));
}

describe('the menu bridge', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    menu.handlers = null;
    resetChats();
  });
  afterEach(() => {
    cleanupModelEvents();
    for (const id of [7, 8, 9]) serve(id, 'stopped');
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

  it('Settings opens the settings place on General; Add Model adds a file and Downloads finds on Hugging Face', async () => {
    const { at, router, fire, stop } = await bridged('/models/7');
    await fire(MENU_EVENTS.OPEN_SETTINGS);
    expect(at()).toBe('/settings/general');
    // A place of its own: Back returns to the one it was opened from.
    expect(router.state.historyAction).toBe('PUSH');
    await fire(MENU_EVENTS.ADD_MODEL_FILE);
    expect(at()).toBe('/library?add=file');
    await fire(MENU_EVENTS.SHOW_DOWNLOADS);
    expect(at()).toBe('/library?add=browse');
    stop();
  });

  it("Chat goes to the selected model's server, else the first running, else says there is none", async () => {
    const { at, router, fire, stop } = await bridged('/models/7');
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/models/7');
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/no servers/i), 'warning');

    // No chat on it yet: an empty one on it.
    serve(9);
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/chat/new?model=9');

    // Its latest chat, where it has one.
    transport.listConversations.mockResolvedValue([
      { id: 4, title: 'older', model_id: 7, system_prompt: null, created_at: '2026-11-01 09:00:00', updated_at: '2026-11-01 09:00:00' },
      { id: 6, title: 'newer', model_id: 7, system_prompt: null, created_at: '2026-11-02 09:00:00', updated_at: '2026-11-02 09:00:00' },
    ]);
    resetChats();
    serve(7);
    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/chat/6');
    stop();
  });

  const press = (init: KeyboardEventInit) =>
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', init));
    });

  it('Ctrl+N starts a chat from anywhere off a Mac, ⌘N does not, until the bridge stops', async () => {
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
    vi.restoreAllMocks();
  });

  it('⌘N starts a chat on a Mac, and Ctrl+N is left to the text field it is a key of', async () => {
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('MacIntel');
    const { at, stop } = await bridged('/models/7');
    press({ key: 'n', ctrlKey: true });
    expect(at()).toBe('/models/7');
    press({ key: 'n', metaKey: true });
    expect(at()).toBe('/chat/new');
    stop();
    vi.restoreAllMocks();
  });

  it('Start opens the picked model’s Load with options sheet, and does nothing with none picked', async () => {
    const { at, router, fire, stop } = await bridged('/library');
    await fire(MENU_EVENTS.START_SERVER);
    expect(at()).toBe('/library');
    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.START_SERVER);
    expect(at()).toBe('/models/7?serve=1');
    stop();
  });

  it('Stop stops the selected running model, and nothing with another model picked', async () => {
    serve(7);
    const { at, router, fire, stop } = await bridged('/models/9');
    await fire(MENU_EVENTS.STOP_SERVER);
    expect(transport.stopServer).not.toHaveBeenCalled();

    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.STOP_SERVER);
    expect(transport.stopServer).toHaveBeenCalledWith(7);
    expect(at()).toBe('/models/7');
    expect(menu.syncMenuStateSilent).toHaveBeenCalledTimes(1);
    stop();
  });

  it("on a chat the menu has no model: a chat's address names its conversation, so Stop stops nothing", async () => {
    serve(7);
    const { at, fire, stop } = await bridged('/chat/7');
    expect(menu.setSelectedModel).toHaveBeenLastCalledWith(null);

    await fire(MENU_EVENTS.STOP_SERVER);

    expect(transport.stopServer).not.toHaveBeenCalled();
    expect(at()).toBe('/chat/7');
    stop();
  });

  it('Remove asks first, removes on Remove, and takes the address back to the library', async () => {
    const { at, stop } = await bridged('/models/7');
    await waitFor(() => expect(transport.listModels).toHaveBeenCalled());
    const user = userEvent.setup();
    const heading = () => screen.queryByRole('heading', { name: 'Remove "qwen3-8b"?' });

    const declined = menu.handlers?.[MENU_EVENTS.REMOVE_MODEL]?.();
    await waitFor(() => expect(heading()).toBeInTheDocument());
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

    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/could not remove qwen3-8b/i), 'error');
    expect(at()).toBe('/models/7');
    expect(menu.syncMenuStateSilent).not.toHaveBeenCalled();
    stop();
  });

  it('Refresh reads the library again; the proxy items act and say so', async () => {
    const { fire, stop } = await bridged('/library');
    await waitFor(() => expect(transport.listModels).toHaveBeenCalledTimes(1));
    await fire(MENU_EVENTS.REFRESH_MODELS);
    await waitFor(() => expect(transport.listModels).toHaveBeenCalledTimes(2));

    await fire(MENU_EVENTS.START_PROXY);
    expect(notify).toHaveBeenCalledWith('Proxy started on port 8080', 'success');
    await fire(MENU_EVENTS.PROXY_STOPPED);
    expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    expect(notify).toHaveBeenCalledWith('Proxy stopped', 'success');
    stop();
  });
});
