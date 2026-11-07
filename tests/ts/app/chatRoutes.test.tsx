/**
 * The chat routes: a chat is addressed by its conversation, and its model is
 * read from it. `/chat/:id` opens that conversation on its model (the
 * paired machine's, for one made for it); `/chat/new` opens an empty one on
 * `?model=` or `?far=`, else on the last model used, the default, or the
 * first loaded, and the first send's conversation replaces the address
 * without remounting the place; a local chat on a model nothing serves goes
 * to its inspector; `/chat/far/:id` is the paired machine's own chat.
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
}
// The place is a placard naming what it was given, with the two calls it makes up.
vi.mock('../../../src/places/chat/ChatPlace', () => ({
  default: function Place({ modelName, conversationId, serverPort, paired, desk, onOpened, onDeleted }: Placard) {
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
        <button onClick={() => onDeleted?.()}>delete it</button>
      </div>
    );
  },
}));

import { HANDED, peekChatHandoff, setChatHandoff } from '../../../src/app/chatHandoff';
import { refreshChats, resetChats } from '../../../src/services/chatRegistry';
import { IDLE_STATUS, applyRemoteStatus, ingestRemoteEvent, resetRemoteState } from '../../../src/services/remoteRegistry';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { renderApp } from '../places/harness';

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

describe('the chat routes', () => {
  const served: number[] = [];
  beforeEach(() => {
    resetRemoteState();
    resetChats();
    daemon.chats = [];
    daemon.settings = {};
    mounts.count = 0;
  });
  afterEach(() => {
    resetRemoteState();
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

  it('a chat on a model nothing serves goes to its inspector', async () => {
    daemon.chats = [chat(3, 23)];
    const { at } = renderApp('/chat/3');
    await waitFor(() => expect(at()).toBe('/models/23'));
    expect(screen.getByRole('main', { name: 'Library' })).toBeInTheDocument();
  });

  it('a conversation the daemon does not list, read again, opens an empty chat', async () => {
    serving(24, 5024);
    const { at } = renderApp('/chat/77');
    await waitFor(() => expect(at()).toBe('/chat/new'));
    expect(transport.listConversations.mock.calls.length).toBeGreaterThanOrEqual(2);
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

  it('/chat/new alone opens on the last model used, if loaded', async () => {
    serving(27, 5027);
    serving(28, 5028);
    daemon.settings = { defaultModelId: 28 };
    daemon.chats = [chat(1, null, '2026-11-03 09:00:00'), chat(2, 27, '2026-11-02 09:00:00'), chat(3, 28, '2026-11-01 09:00:00')];
    renderApp('/chat/new');
    expect(await page()).toHaveAttribute('data-port', '5027');
  });

  it('then the default model, if loaded, when the last one used is not', async () => {
    serving(29, 5029);
    serving(30, 5030);
    daemon.settings = { defaultModelId: 30 };
    daemon.chats = [chat(2, 31)];
    renderApp('/chat/new');
    expect(await page()).toHaveAttribute('data-port', '5030');
  });

  it('then the first loaded model, when neither is', async () => {
    serving(32, 5032);
    daemon.settings = { defaultModelId: 33 };
    daemon.chats = [chat(2, 31)];
    renderApp('/chat/new');
    expect(await page()).toHaveAttribute('data-port', '5032');
  });

  it('with nothing loaded, the inspector of the last model used, where Load is', async () => {
    daemon.settings = { defaultModelId: 33 };
    daemon.chats = [chat(2, 34)];
    const { at } = renderApp('/chat/new');
    await waitFor(() => expect(at()).toBe('/models/34'));
  });

  it('with nothing loaded and nothing used, the default model\'s inspector, else the library', async () => {
    daemon.settings = { defaultModelId: 33 };
    const first = renderApp('/chat/new');
    await waitFor(() => expect(first.at()).toBe('/models/33'));
    first.unmount();

    daemon.settings = {};
    resetChats();
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
    const { at, router } = renderApp('/chat/3');
    await waitFor(() => expect(at()).toBe('/models/36'));
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
