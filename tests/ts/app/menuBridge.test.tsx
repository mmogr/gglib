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
vi.mock('../../../src/pages/ModelControlCenterPage', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/app/ChatRoute', () => ({ ChatRoute: () => <main aria-label="Chat" />, CHAT_LOADING: null }));

import { startMenuBridge } from '../../../src/app/menuBridge';
import { MENU_EVENTS } from '../../../src/services/platform/menuEvents';
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
  });
  afterEach(() => {
    cleanupModelEvents();
    for (const id of [7, 8, 9]) serve(id, 'stopped');
  });

  it('tells the menu the selected model on every move, once per change', async () => {
    const { at, router, stop } = await bridged('/models/7');
    expect(menu.setSelectedModel).toHaveBeenLastCalledWith(7);
    expect(menu.setSelectedModel).toHaveBeenCalledTimes(1);

    await act(() => router.navigate('/models/7?settings=general'));
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

  it('Settings opens the dialog over the place; Add Model and Downloads open the Add pane', async () => {
    const { at, fire, stop } = await bridged('/models/7');
    await fire(MENU_EVENTS.OPEN_SETTINGS);
    expect(at()).toBe('/models/7?settings=general');
    await fire(MENU_EVENTS.ADD_MODEL_FILE);
    expect(at()).toBe('/library?view=add&sub=add');
    await fire(MENU_EVENTS.SHOW_DOWNLOADS);
    expect(at()).toBe('/library?view=add&sub=browse');
    stop();
  });

  it("Chat goes to the selected model's server, else the first running, else says there is none", async () => {
    const { at, router, fire, stop } = await bridged('/models/7');
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/models/7');
    expect(notify).toHaveBeenCalledWith(expect.stringMatching(/no servers/i), 'warning');

    serve(9);
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/chat/9');

    serve(7);
    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.SHOW_CHAT);
    expect(at()).toBe('/chat/7');
    stop();
  });

  it('Start opens the picked model’s Serve dialog, and does nothing with none picked', async () => {
    const { at, router, fire, stop } = await bridged('/library');
    await fire(MENU_EVENTS.START_SERVER);
    expect(at()).toBe('/library');
    await act(() => router.navigate('/models/7'));
    await fire(MENU_EVENTS.START_SERVER);
    expect(at()).toBe('/models/7?serve=1');
    stop();
  });

  it('Stop stops the selected running model and leaves its chat for its inspector', async () => {
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
