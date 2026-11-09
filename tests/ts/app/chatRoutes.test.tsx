/**
 * The chat routes: a chat is addressed by its conversation, and its model is
 * read from it. `/chat/:id` opens that conversation on its model (the
 * paired machine's, for one made for it); `/chat/new` opens an empty one on
 * `?model=` or `?far=`, else on the last model used or the default while
 * the library has it, or the first loaded, and the first send's
 * conversation replaces the address without remounting the place; a local
 * chat on a model nothing serves opens all the same, with no port (a send
 * loads the model), and one whose model stops while it is open stays; the
 * composer's Show <model>, Show log and a paired model's new chat navigate;
 * `/chat/far/:id` is the paired machine's own chat.
 *
 * Each test serves its own model ids: the server registry outlives a test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { useEffect } from 'react';

import { farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

interface Chat {
  id: number;
  title: string;
  model_id: number | null;
  system_prompt: null;
  created_at: string;
  updated_at: string;
  settings?: object;
}
const daemon = vi.hoisted(() => ({ chats: [] as Chat[], settings: {} as object }));
const mounts = vi.hoisted(() => ({ count: 0 }));

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => []),
  listPairedModels: vi.fn(async () => pairedModels([farEntry('qwen3-8b', 7)])),
  getSettings: vi.fn(async () => daemon.settings),
  listConversations: vi.fn(async () => daemon.chats.map((c) => ({ ...c }))),
  // Read only by a test that bridges the server list, as the app does at start.
  listServers: vi.fn(async (): Promise<unknown[]> => []),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

interface Placard {
  modelName: string;
  conversationId: number | null;
  serverPort?: number;
  paired?: { far: { id: number }; machineName: string };
  desk?: { machineName: string };
  onOpened?: (id: number) => void;
  onDeleted?: () => void;
  onShowModel?: () => void;
  onShowLog?: () => void;
  onOpenFar?: (id: number) => void;
}
// The place is a placard naming what it was given, with the two calls it makes up.
vi.mock('../../../src/places/chat/ChatPlace', () => ({
  default: function Place({ modelName, conversationId, serverPort, paired, desk, onOpened, onDeleted, onShowModel, onShowLog, onOpenFar }: Placard) {
    useEffect(() => {
      mounts.count += 1;
    }, []);
    return (
      <div
        data-testid="chat-page"
        data-conversation={conversationId ?? 'none'}
        data-port={serverPort ?? ''}
        data-paired={paired ? `${paired.machineName}:${paired.far.id}` : ''}
        data-desk={desk?.machineName ?? ''}
      >
        Chatting with {modelName}
        <button onClick={() => onOpened?.(5)}>first send</button>
        {/* A run's end opens the chat it ended in, the one already open. */}
        <button onClick={() => conversationId !== null && onOpened?.(conversationId)}>run ends</button>
        <button onClick={() => onDeleted?.()}>delete it</button>
        {onShowModel && <button onClick={onShowModel}>show the model</button>}
        {onShowLog && <button onClick={onShowLog}>show its log</button>}
        {onOpenFar && <button onClick={() => onOpenFar(7)}>a desk model</button>}
      </div>
    );
  },
}));

import { HANDED, peekChatHandoff, setChatHandoff } from '../../../src/app/chatHandoff';
import { applyLibraryRead, resetModelLibrary } from '../../../src/services/modelRegistry';
import type { GgufModel } from '../../../src/types';
import { refreshChats, resetChats } from '../../../src/services/chatRegistry';
import { IDLE_STATUS, applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from '../../../src/services/remoteRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { cleanupServerEvents, forgetServerList, initServerEvents } from '../../../src/services/serverEvents';
import { createAppRouter } from '../../../src/app/router';
import { PlaceProviders, renderApp } from '../places/harness';
import { Outlet, RouterProvider } from 'react-router';
import { render } from '@testing-library/react';

const DESK = '3ca82708b995';

function connectedTo(fingerprint: string, name: string) {
  return {
    ...IDLE_STATUS,
    connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: fingerprint, path: 'direct' as const, away_for_s: null },
    paired_name: name,
  };
}

function chat(id: number, model: number | null, updated_at = '2026-11-01 09:00:00', settings?: object): Chat {
  return { id, title: `chat ${id}`, model_id: model, system_prompt: null, created_at: updated_at, updated_at, ...(settings && { settings }) };
}

let clock = Date.now();
function serve(modelId: number, port: number, modelName = `model ${modelId}`) {
  act(() => ingestServerEvent({ type: 'running', modelId: String(modelId), port, updatedAt: ++clock, modelName }));
}
function stop(modelId: number) {
  act(() => ingestServerEvent({ type: 'stopped', modelId: String(modelId), port: 0, updatedAt: ++clock }));
}

const page = () => screen.findByTestId('chat-page');

/** The library, as its first read answers it: models `ids`. */
function library(...ids: number[]) {
  act(() => applyLibraryRead({ models: ids.map((id) => ({ id, name: `model ${id}` }) as GgufModel) }));
}

describe('the chat routes', () => {
  const served: number[] = [];
  beforeEach(() => {
    resetRemoteState();
    resetChats();
    daemon.chats = [];
    daemon.settings = {};
    mounts.count = 0;
    library();
    // The app's server bridge, as `AppContent` starts it: the list it reads
    // is what lets a chat say nothing serves its model.
    forgetServerList();
    initServerEvents();
  });
  afterEach(() => {
    cleanupServerEvents();
    resetRemoteState();
    resetModelLibrary();
    // Nothing a test served stays running for the next.
    served.splice(0).forEach(stop);
  });
  const serving = (modelId: number, port: number, modelName?: string) => {
    served.push(modelId);
    serve(modelId, port, modelName);
  };

  it('/chat/:id opens that conversation on the model it names, served here', async () => {
    serving(21, 5021, 'qwen3-8b');
    daemon.chats = [chat(3, 21)];
    renderApp('/chat/3');
    const chatPage = await page();
    expect(chatPage).toHaveAttribute('data-conversation', '3');
    expect(chatPage).toHaveAttribute('data-port', '5021');
    expect(chatPage).toHaveTextContent('Chatting with qwen3-8b');
  });

  it("reads a conversation's model from its settings before its model_id", async () => {
    serving(22, 5022, 'gemma-3-12b');
    serving(99, 5099);
    daemon.chats = [chat(3, 99, undefined, { model: { machine: { kind: 'local' }, id: 22 } })];
    renderApp('/chat/3');
    expect(await page()).toHaveAttribute('data-port', '5022');
  });

  it('a chat on a model nothing serves opens all the same, with no port: a send loads it', async () => {
    library(23);
    daemon.chats = [chat(3, 23)];
    const { at } = renderApp('/chat/3');
    const chatPage = await page();
    expect(at()).toBe('/chat/3');
    expect(chatPage).toHaveAttribute('data-conversation', '3');
    expect(chatPage).toHaveAttribute('data-port', '');
    expect(chatPage).toHaveTextContent('Chatting with model 23');
  });

  describe('on a cold load, before the server list is heard', () => {
    let answer: (list: unknown[]) => void = () => {};
    beforeEach(() => {
      // A fresh start, with the list still out.
      cleanupServerEvents();
      forgetServerList();
      transport.listServers.mockImplementation(() => new Promise<unknown[]>((resolve) => (answer = resolve)));
      initServerEvents();
    });
    afterEach(() => {
      transport.listServers.mockImplementation(async () => []);
    });

    it('waits in the order the app starts: the bridge from the layout\'s effect, after the routes have rendered', async () => {
      cleanupServerEvents();
      forgetServerList();
      // As `AppContent`: the place renders first, and the bridge starts in the layout's effect.
      function AppOrder() {
        useEffect(() => {
          initServerEvents();
        }, []);
        return (
          <PlaceProviders>
            <Outlet />
          </PlaceProviders>
        );
      }
      const router = createAppRouter(<AppOrder />, ['/chat/new?model=31']);
      render(<RouterProvider router={router} />);
      await act(async () => {});
      expect(`${router.state.location.pathname}${router.state.location.search}`).toBe('/chat/new?model=31');
      served.push(31);
      await act(async () => answer([listed(31, 5031)]));
      await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5031'));
    });

    it('reads the list again after reads that failed, waiting longer each time, rather than wait for ever', async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      cleanupServerEvents();
      forgetServerList();
      const busy = async () => {
        throw new Error('daemon busy');
      };
      // The first read and the first retry fail; the second retry lands.
      transport.listServers.mockClear();
      transport.listServers.mockImplementationOnce(busy).mockImplementationOnce(busy).mockImplementation(async () => [listed(32, 5032)]);
      initServerEvents();
      served.push(32);
      const { at } = renderApp('/chat/new?model=32');
      await act(async () => {});
      expect(at()).toBe('/chat/new?model=32');
      await act(async () => {
        await vi.advanceTimersByTimeAsync(1100);
      });
      expect(transport.listServers).toHaveBeenCalledTimes(2);
      // The chat is open meanwhile: it needs no port to send.
      expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '');
      await act(async () => {
        await vi.advanceTimersByTimeAsync(2100);
      });
      expect(await page()).toHaveAttribute('data-port', '5032');
      expect(at()).toBe('/chat/new?model=32');
      vi.useRealTimers();
    });
    const listed = (modelId: number, port: number) => ({
      model_id: modelId, model_name: 'qwen3', pid: 1, port, started_at: Math.floor(++clock / 1000), runtime: 'llama',
    });

    it('opens the chat on the model it names at once, and takes the port once the list says it is served', async () => {
      const { at } = renderApp('/chat/new?model=27');
      expect(await page()).toHaveAttribute('data-port', '');

      served.push(27);
      await act(async () => answer([listed(27, 5027)]));
      await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5027'));
      expect(at()).toBe('/chat/new?model=27');
    });

    it('stays open with no port once the list it waited for serves nothing', async () => {
      const { at } = renderApp('/chat/new?model=28');
      await act(async () => {});
      await act(async () => answer([]));
      expect(await page()).toHaveAttribute('data-port', '');
      expect(at()).toBe('/chat/new?model=28');
    });

    it('a new chat with no model named waits for the list before picking the first loaded', async () => {
      const { at } = renderApp('/chat/new');
      await act(async () => {});
      expect(at()).toBe('/chat/new');
      expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
      served.push(29);
      await act(async () => answer([listed(29, 5029)]));
      expect(await page()).toHaveAttribute('data-port', '5029');
    });
  });

  it('a chat whose model stops while it is open stays open on the port it had', async () => {
    serving(25, 5025, 'qwen3-8b');
    daemon.chats = [chat(3, 25)];
    const { at } = renderApp('/chat/3');
    expect(await page()).toHaveAttribute('data-port', '5025');
    const mounted = mounts.count;

    stop(25);
    served.splice(served.indexOf(25), 1);

    expect(at()).toBe('/chat/3');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5025');
    expect(mounts.count).toBe(mounted);
  });

  it("a local chat's Show <model> opens its inspector, Show log its log, and a paired model a new chat on it", async () => {
    const user = userEvent.setup();
    serving(26, 5026);
    daemon.chats = [chat(3, 26)];
    const view = renderApp('/chat/3');
    await page();
    await user.click(screen.getByRole('button', { name: 'show the model' }));
    await waitFor(() => expect(view.at()).toBe('/models/26'));
    view.unmount();

    const again = renderApp('/chat/3');
    await page();
    await user.click(screen.getByRole('button', { name: 'show its log' }));
    await waitFor(() => expect(again.at()).toBe('/models/26/log'));
    again.unmount();

    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    const third = renderApp('/chat/3');
    await page();
    await user.click(screen.getByRole('button', { name: 'a desk model' }));
    await waitFor(() => expect(third.at()).toBe('/chat/new?far=7'));
    expect(await page()).toHaveAttribute('data-paired', 'desk:7');
  });

  it('a conversation the daemon does not list, read again, opens an empty chat', async () => {
    serving(24, 5024);
    const { at } = renderApp('/chat/77');
    await waitFor(() => expect(at()).toBe('/chat/new'));
    expect(transport.listConversations.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it('a conversation missing from a first list made before it is found by the read again', async () => {
    serving(44, 5044);
    daemon.chats = [chat(78, 44)];
    // The first read predates it; the route's own read again lists it.
    transport.listConversations.mockResolvedValueOnce([]);
    const { at } = renderApp('/chat/78');
    expect(await page()).toHaveAttribute('data-conversation', '78');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5044');
    expect(at()).toBe('/chat/78');
  });

  it('a list that could not be read is read again, and the chat opens on its own model, never a guessed one', async () => {
    serving(45, 5045);
    serving(46, 5046);
    // The last one used is on 45: a guess would open there.
    daemon.chats = [chat(80, 45, '2026-11-03 09:00:00'), chat(79, 46, '2026-11-01 09:00:00')];
    transport.listConversations.mockRejectedValueOnce(new Error('daemon busy'));
    const { at } = renderApp('/chat/79');
    expect(await page()).toHaveAttribute('data-conversation', '79');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5046');
    expect(mounts.count).toBe(1);
    expect(at()).toBe('/chat/79');
  });

  it('a list that still cannot be read says so with Try again, opens nothing, and opens the chat once it is read', async () => {
    const user = userEvent.setup();
    serving(45, 5045);
    serving(46, 5046);
    // The last one used is on 45: a guess would open there.
    daemon.chats = [chat(80, 45, '2026-11-03 09:00:00'), chat(79, 46, '2026-11-01 09:00:00')];
    transport.listConversations.mockRejectedValue(new Error('daemon gone'));
    try {
      const { at } = renderApp('/chat/79');
      expect(await screen.findByText(/The chat couldn't be read/)).toBeInTheDocument();
      expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
      expect(at()).toBe('/chat/79');
      transport.listConversations.mockImplementation(async () => daemon.chats.map((c) => ({ ...c })));
      await user.click(screen.getByRole('button', { name: 'Try again' }));
      expect(await page()).toHaveAttribute('data-port', '5046');
    } finally {
      transport.listConversations.mockImplementation(async () => daemon.chats.map((c) => ({ ...c })));
    }
  });

  it("/chat/new?model= opens an empty chat on it, and the first send's conversation replaces the address without remounting the place", async () => {
    const user = userEvent.setup();
    serving(25, 5025, 'qwen3-8b');
    serving(26, 5026, 'other');
    // The conversation the send makes has run on nothing yet, and the last one used is another model.
    daemon.chats = [chat(5, null, '2026-11-02 09:00:00'), chat(4, 26, '2026-11-01 09:00:00')];
    const { at, router } = renderApp('/chat/new?model=25');
    expect(await page()).toHaveAttribute('data-conversation', 'none');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5025');

    await user.click(screen.getByRole('button', { name: 'first send' }));

    expect(at()).toBe('/chat/5');
    expect(router.state.historyAction).toBe('REPLACE');
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-conversation', '5'));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5025');
    expect(mounts.count).toBe(1);
  });

  it('a chat this route opened goes by its own model once a run has named one', async () => {
    const user = userEvent.setup();
    serving(42, 5042);
    serving(43, 5043);
    daemon.chats = [chat(5, null, '2026-11-02 09:00:00'), chat(6, 42, '2026-11-01 09:00:00')];
    const { router } = renderApp('/chat/new?model=42');
    await user.click(await screen.findByRole('button', { name: 'first send' }));
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-conversation', '5'));

    // Moved to 43 and run there since; the list is read again on the way.
    daemon.chats = [chat(5, 43, '2026-11-03 09:00:00'), chat(6, 42, '2026-11-01 09:00:00')];
    await act(() => router.navigate('/chat/6'));
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-conversation', '6'));
    await act(() => refreshChats('this'));
    await act(() => router.navigate('/chat/5'));

    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5043'));
  });

  it('/chat/new alone opens on the last model used, loaded or not', async () => {
    library(27, 28);
    serving(28, 5028);
    daemon.settings = { defaultModelId: 28 };
    daemon.chats = [chat(1, null, '2026-11-03 09:00:00'), chat(2, 27, '2026-11-02 09:00:00'), chat(3, 28, '2026-11-01 09:00:00')];
    renderApp('/chat/new');
    const chatPage = await page();
    expect(chatPage).toHaveTextContent('Chatting with model 27');
    expect(chatPage).toHaveAttribute('data-port', '');
  });

  it('then the default model, loaded or not, when the library no longer has the last one used', async () => {
    library(30);
    serving(29, 5029);
    daemon.settings = { defaultModelId: 30 };
    daemon.chats = [chat(2, 31)];
    renderApp('/chat/new');
    expect(await page()).toHaveTextContent('Chatting with model 30');
  });

  it('then the first loaded model, when the library has neither', async () => {
    serving(32, 5032);
    daemon.settings = { defaultModelId: 33 };
    daemon.chats = [chat(2, 31)];
    renderApp('/chat/new');
    expect(await page()).toHaveAttribute('data-port', '5032');
  });

  it('waits for the library before it picks', async () => {
    resetModelLibrary();
    daemon.chats = [chat(2, 34)];
    const { at } = renderApp('/chat/new');
    await act(async () => {});
    expect(screen.queryByTestId('chat-page')).not.toBeInTheDocument();
    library(34);
    expect(await page()).toHaveTextContent('Chatting with model 34');
    expect(at()).toBe('/chat/new');
  });

  it('with nothing used, no default and nothing loaded, the library', async () => {
    daemon.settings = { defaultModelId: 33 };
    const { at } = renderApp('/chat/new');
    await waitFor(() => expect(at()).toBe('/library'));
  });

  it('deleting the open chat opens an empty one on its model', async () => {
    const user = userEvent.setup();
    serving(35, 5035);
    daemon.chats = [chat(3, 35)];
    const { at } = renderApp('/chat/3');
    await user.click(await screen.findByRole('button', { name: 'delete it' }));
    await waitFor(() => expect(at()).toBe('/chat/new?model=35'));
  });

  it('a handoff is read only by the navigation that set it, and taken once the route has it', async () => {
    daemon.chats = [chat(3, 36)];
    const handoff = { modelId: 36, port: 5555, modelName: 'qwen3-8b' };
    setChatHandoff(handoff);
    // A plain visit: the address alone, not handed, so nothing serves 36.
    const { router } = renderApp('/chat/3');
    expect(await page()).toHaveAttribute('data-port', '');
    expect(peekChatHandoff()).toBe(handoff);

    await act(() => router.navigate('/chat/3', { state: HANDED }));

    const chatPage = await page();
    expect(chatPage).toHaveAttribute('data-port', '5555');
    expect(chatPage).toHaveTextContent('Chatting with qwen3-8b');
    expect(peekChatHandoff()).toBeNull();
  });

  it("a model switch's handoff opens the conversation on the new model", async () => {
    serving(37, 5037);
    serving(38, 5038, 'gemma');
    daemon.chats = [chat(3, 37)];
    const { router } = renderApp('/chat/3');
    expect(await page()).toHaveAttribute('data-port', '5037');

    setChatHandoff({ modelId: 38, port: 5038, modelName: 'gemma' });
    await act(() => router.navigate('/chat/3', { state: HANDED }));
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5038'));
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-conversation', '3');
  });

  it('a run that ends after a model switch keeps the chat on the model it was moved to, the old one unloaded', async () => {
    const user = userEvent.setup();
    serving(46, 5046);
    serving(47, 5047, 'gemma');
    daemon.chats = [chat(3, 46)];
    const { at, router } = renderApp('/chat/3');
    expect(await page()).toHaveAttribute('data-port', '5046');

    setChatHandoff({ modelId: 47, port: 5047, modelName: 'gemma' });
    await act(() => router.navigate('/chat/3', { state: HANDED }));
    await waitFor(() => expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5047'));
    stop(46);
    const mounted = mounts.count;

    // The list still names 46: no run has written 47 to it yet.
    await user.click(screen.getByRole('button', { name: 'run ends' }));

    expect(at()).toBe('/chat/3');
    expect(screen.getByTestId('chat-page')).toHaveAttribute('data-port', '5047');
    expect(mounts.count).toBe(mounted);
  });

  it("/chat/new?far= opens an empty chat on the paired machine's model", async () => {
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    renderApp('/chat/new?far=7');
    const chatPage = await page();
    expect(chatPage).toHaveTextContent('Chatting with qwen3-8b');
    expect(chatPage).toHaveAttribute('data-paired', 'desk:7');
    expect(chatPage).toHaveAttribute('data-conversation', 'none');
  });

  it("a conversation made for the paired machine's model opens on it, keeps across a drop and an unnamed connection, and closes once another machine answers", async () => {
    daemon.chats = [chat(3, null, undefined, { model: { machine: { kind: 'paired', fingerprint: DESK }, id: 7 } })];
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    const { at } = renderApp('/chat/3');
    const chatPage = await page();
    expect(chatPage).toHaveAttribute('data-paired', 'desk:7');
    expect(chatPage).toHaveAttribute('data-conversation', '3');

    act(() => ingestRemoteEvent({ type: 'remote_disconnected' }));
    expect(screen.getByTestId('chat-page')).toBeInTheDocument();
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    act(() => ingestRemoteEvent({ type: 'remote_joined', port: 41235 }));
    expect(screen.getByTestId('chat-page')).toBeInTheDocument();
    expect(at()).toBe('/chat/3');

    act(() => applyRemoteStatus(connectedTo('ffeeddccbbaa', 'study')));
    await waitFor(() => expect(at()).toBe('/library'));
  });

  it('a conversation made for another machine than the one connected does not open', async () => {
    daemon.chats = [chat(3, null, undefined, { model: { machine: { kind: 'paired', fingerprint: 'ffeeddccbbaa' }, id: 7 } })];
    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    const { at } = renderApp('/chat/3');
    await waitFor(() => expect(at()).toBe('/library'));
  });

  it("/chat/far/:id is the paired machine's own chat, which needs no port; with no machine connected, the library", async () => {
    const first = renderApp('/chat/far/4');
    await waitFor(() => expect(first.at()).toBe('/library'));
    first.unmount();

    act(() => applyRemoteStatus(connectedTo(DESK, 'desk')));
    renderApp('/chat/far/4');
    const chatPage = await page();
    expect(chatPage).toHaveAttribute('data-desk', 'desk');
    expect(chatPage).toHaveAttribute('data-conversation', '4');
    expect(chatPage).toHaveAttribute('data-port', '');
  });
});
