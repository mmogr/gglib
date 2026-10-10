/**
 * A chat's title on the chat page. A chat still called `New Chat` is titled
 * by itself, once for each time it is opened, when its first reply finishes
 * there: a run the page started, or one it found going and read to its end.
 * Opening a chat asks for nothing, whatever it already holds; nor does a
 * reply that was stopped, or one that ended while another chat was open; nor
 * a chat whose title somebody chose. The head's button asks whenever it is
 * pressed, after a question when it would replace such a title. A title that
 * arrives after its chat was left names it in the list, and opens nothing.
 *
 * The daemon here ends a run as the real one does: the reply is saved first,
 * marked unfinished unless the run completed, and only then does the run
 * read as ended.
 *
 * "Nothing was asked" is never read off the moment a reply lands: each test
 * goes on to something the person does next, and counts every request then.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../../../src/services/transport';
import type { ChatMessage, GenerateTitleParams } from '../../../src/services/transport';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { RunInfo } from '../../../src/types/generated/RunInfo';
import { agentRun, chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

type User = ReturnType<typeof userEvent.setup>;

let fixture: ChatFixture;
let nextRow: number;
/** The runs still going, by id: what ends each, and its end as its readers wait for it. */
let going: Map<string, { end: (info: RunInfo) => void; ended: Promise<RunInfo> }>;

type Stub = ReturnType<typeof pageTransport>;
const stub = () => transport.current as Stub;

function save(conversationId: number, role: 'user' | 'assistant', content: string, metadata?: ChatMessage['metadata']) {
  const row: ChatMessage = { id: nextRow++, conversation_id: conversationId, role, content, created_at: '2026-10-08T09:00:00Z' };
  fixture.rows[conversationId] = [...(fixture.rows[conversationId] ?? []), metadata ? { ...row, metadata } : row];
}

/** A question and its finished reply, already saved in `conversationId`. */
function exchange(conversationId: number, asked: string, reply: string) {
  save(conversationId, 'user', asked);
  save(conversationId, 'assistant', reply);
}

/** A run going in `conversationId`, listed as the daemon lists it until it ends. */
function run(id: string, conversationId: number) {
  let end: (info: RunInfo) => void = () => {};
  const ended = new Promise<RunInfo>((resolve) => { end = resolve; });
  going.set(id, { end, ended });
  fixture.runs = [agentRun(id, conversationId, 'in_progress'), ...fixture.runs];
}

/** End run `id`: its reply is saved, unfinished unless it completed, and then it reads as ended. */
function end(id: string, status: 'completed' | 'cancelled' | 'failed', reply: string) {
  const conversationId = fixture.runs.find((r) => r.id === id)!.conversation_id!;
  save(conversationId, 'assistant', reply, status === 'completed' ? undefined : { incomplete: true });
  fixture.runs = fixture.runs.map((r) => (r.id === id ? agentRun(id, conversationId, status) : r));
  going.get(id)!.end(agentRun(id, conversationId, status));
}

function pageTransport() {
  return {
    ...chatTransport(fixture),
    startAgentRun: vi.fn(async (id: string, request: AgentRunRequest) => {
      const conversationId = request.conversation_id as number;
      save(conversationId, 'user', String(request.messages.at(-1)?.content ?? ''));
      run(id, conversationId);
      return agentRun(id, conversationId, 'queued');
    }),
    // What the run has written so far, then nothing until it ends or its reader leaves.
    readRunEvents: async function* (id: string, _after: number, signal: AbortSignal) {
      let seq = 0;
      for (const frame of fixture.frames[id] ?? []) yield { type: 'frame' as const, seq: ++seq, data: JSON.stringify(frame) };
      const left = new Promise<null>((resolve) => signal.addEventListener('abort', () => resolve(null)));
      const info = await Promise.race([going.get(id)!.ended, left]);
      if (info) yield { type: 'end' as const, info };
    },
    // Stop: the run ends cancelled, with what it had written saved as unfinished.
    cancelRun: vi.fn(async (id: string) => {
      const info = fixture.runs.find((r) => r.id === id)!;
      end(id, 'cancelled', 'Two plus');
      return info;
    }),
    generateChatTitle: vi.fn(async (_params: GenerateTitleParams) => 'Sums'),
    updateConversationTitle: vi.fn(async (id: number, title: string) => {
      fixture.conversations = fixture.conversations.map((c) => (c.id === id ? { ...c, title } : c));
    }),
  };
}

/** The conversation each title request was made for, in order. */
const titled = () => stub().generateChatTitle.mock.calls.map(([params]) => params.messages[0].conversation_id);

const stopButton = () => screen.queryByRole('button', { name: 'Stop' });
const titleButton = () => screen.getByRole('button', { name: 'Generate title with AI' });

/** Render the page and wait for the conversation it opens on: its rows asked for, and shown. */
async function renderPage() {
  render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
  await waitFor(() => expect(stub().getThread).toHaveBeenCalled());
  await waitFor(() => expect(screen.queryByText('Loading messages…')).not.toBeInTheDocument());
}

/** Send `text` from the composer; answers the id of the run it started, still going. */
async function ask(user: User, text: string): Promise<string> {
  const runs = stub().startAgentRun.mock.calls.length;
  await user.type(screen.getByRole('textbox', { name: 'Message' }), `${text}{Enter}`);
  await waitFor(() => expect(stub().startAgentRun).toHaveBeenCalledTimes(runs + 1));
  await waitFor(() => expect(stopButton()).toBeInTheDocument());
  return stub().startAgentRun.mock.calls[runs][0];
}

/** The reply `text` is shown as saved, and nothing is being written. */
async function settled(text: string) {
  await screen.findByText(text);
  await waitFor(() => expect(stopButton()).not.toBeInTheDocument());
}

/** Open the `nth` conversation of the list, and wait for the reply `text` it holds. */
async function openChat(user: User, nth: number, text: string) {
  await user.click(screen.getAllByRole('option')[nth]);
  await settled(text);
}

beforeEach(() => {
  window.localStorage.clear();
  nextRow = 1;
  going = new Map();
  fixture = { conversations: [conversation(1, 'New Chat')], rows: {}, runs: [], frames: {} };
  transport.current = pageTransport();
});

describe("ChatPage, a chat's title", () => {
  it('titles a new chat once, from its saved rows, when its first reply ends', async () => {
    const user = userEvent.setup();
    await renderPage();
    const id = await ask(user, '2+2?');
    expect(stub().generateChatTitle).not.toHaveBeenCalled();

    end(id, 'completed', '2+2 equals 4.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
    expect(stub().generateChatTitle).toHaveBeenCalledWith({
      serverPort: 4321,
      messages: [
        expect.objectContaining({ role: 'user', content: '2+2?' }),
        expect.objectContaining({ role: 'assistant', content: '2+2 equals 4.' }),
      ],
      prompt: DEFAULT_TITLE_GENERATION_PROMPT,
    });
    expect(stub().updateConversationTitle).toHaveBeenCalledWith(1, 'Sums');
  });

  it('takes a chat with no title at all for a new one', async () => {
    const user = userEvent.setup();
    fixture.conversations = [conversation(1, '')];
    await renderPage();
    end(await ask(user, '2+2?'), 'completed', '2+2 equals 4.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
  });

  it('asks once for an opening: a title that came back as New Chat is not asked for again by the next reply', async () => {
    const user = userEvent.setup();
    stub().generateChatTitle.mockResolvedValue('New Chat');
    await renderPage();
    end(await ask(user, '2+2?'), 'completed', '2+2 equals 4.');
    await waitFor(() => expect(stub().updateConversationTitle).toHaveBeenCalledTimes(1));

    end(await ask(user, 'And 3+3?'), 'completed', '3+3 equals 6.');
    await settled('3+3 equals 6.');

    // The button still asks, and its request is the second there has been.
    await user.click(titleButton());
    await waitFor(() => expect(stub().updateConversationTitle).toHaveBeenCalledTimes(2));
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(2);
  });

  it('asks for nothing when a chat that already has replies is opened, until the button is pressed', async () => {
    const user = userEvent.setup();
    exchange(1, '2+2?', '2+2 equals 4.');
    await renderPage();
    await settled('2+2 equals 4.');

    await user.click(titleButton());

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('Replace conversation title?')).not.toBeInTheDocument();
  });

  it('asks for nothing when a chat is switched to, straight after a reply finished in another, or switched back to', async () => {
    const user = userEvent.setup();
    // Both chats stay `New Chat` throughout, so only the rule holds a request back.
    stub().generateChatTitle.mockResolvedValue('New Chat');
    fixture.conversations.push(conversation(2, 'New Chat'));
    exchange(2, 'Capital of France?', 'Paris.');
    await renderPage();
    end(await ask(user, '2+2?'), 'completed', '2+2 equals 4.');
    await waitFor(() => expect(stub().updateConversationTitle).toHaveBeenCalledTimes(1));

    await openChat(user, 1, 'Paris.');
    await openChat(user, 0, '2+2 equals 4.');

    await user.click(titleButton());
    await waitFor(() => expect(stub().updateConversationTitle).toHaveBeenCalledTimes(2));
    expect(titled()).toEqual([1, 1]);
  });

  it('leaves the open chat open when a title arrives for one that was left', async () => {
    const user = userEvent.setup();
    fixture.conversations.push(conversation(2, 'Parsing GGUF'));
    exchange(2, 'Capital of France?', 'Paris.');
    let answer: (title: string) => void = () => {};
    stub().generateChatTitle.mockImplementation(() => new Promise<string>((resolve) => { answer = resolve; }));
    await renderPage();
    end(await ask(user, '2+2?'), 'completed', '2+2 equals 4.');
    await waitFor(() => expect(stub().generateChatTitle).toHaveBeenCalledTimes(1));
    await openChat(user, 1, 'Paris.');

    answer('Sums');

    // The list that names the chat is the same reading that could have reopened it.
    expect(await screen.findByRole('option', { name: /Sums/ })).toHaveAttribute('aria-selected', 'false');
    expect(screen.getByRole('heading', { name: 'Parsing GGUF' })).toBeInTheDocument();
    expect(screen.getByText('Paris.')).toBeInTheDocument();
  });

  it('leaves a title somebody chose alone, and one changed to New Chat after the reply ended', async () => {
    const user = userEvent.setup();
    fixture.conversations = [conversation(1, 'launchd KeepAlive')];
    await renderPage();
    end(await ask(user, '2+2?'), 'completed', '2+2 equals 4.');
    await settled('2+2 equals 4.');

    // The title was weighed when the reply ended; a later change of it is not a reply ending.
    await user.click(screen.getByRole('button', { name: 'Rename conversation' }));
    const field = screen.getByRole('textbox', { name: 'Conversation title' });
    await user.clear(field);
    await user.type(field, 'New Chat{Enter}');
    await screen.findByRole('heading', { name: 'New Chat' });
    await ask(user, 'And 3+3?');

    expect(stub().generateChatTitle).not.toHaveBeenCalled();
    expect(stub().updateConversationTitle).toHaveBeenCalledWith(1, 'New Chat');
  });

  it('asks for nothing when the first reply is stopped, and titles the chat when the next one finishes', async () => {
    const user = userEvent.setup();
    await renderPage();
    await ask(user, '2+2?');
    await user.click(stopButton()!);
    await settled('Two plus');

    const id = await ask(user, 'Go on.');
    expect(stub().generateChatTitle).not.toHaveBeenCalled();
    end(id, 'completed', '2+2 equals 4.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
  });

  it('asks for nothing when the first reply fails, and titles the chat when the next one finishes', async () => {
    const user = userEvent.setup();
    await renderPage();
    const first = await ask(user, '2+2?');
    end(first, 'failed', 'Two plus');
    await settled('Two plus');

    const id = await ask(user, 'Go on.');
    expect(stub().generateChatTitle).not.toHaveBeenCalled();
    end(id, 'completed', '2+2 equals 4.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
  });

  it('asks for nothing for a reply that ended while another chat was open, and titles the chat when the next one finishes', async () => {
    const user = userEvent.setup();
    fixture.conversations.push(conversation(2, 'New Chat'));
    exchange(2, 'Capital of France?', 'Paris.');
    await renderPage();
    const left = await ask(user, '2+2?');
    await openChat(user, 1, 'Paris.');
    end(left, 'completed', '2+2 equals 4.');
    await openChat(user, 0, '2+2 equals 4.');

    const id = await ask(user, 'And 3+3?');
    expect(stub().generateChatTitle).not.toHaveBeenCalled();
    end(id, 'completed', '3+3 equals 6.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(titled()).toEqual([1]);
  });

  it('titles a chat once when a reply it found going is read to its end', async () => {
    save(1, 'user', '2+2?');
    run('r1', 1);
    fixture.frames.r1 = [{ type: 'text_delta', content: '2+2 equals' }];
    await renderPage();
    await waitFor(() => expect(stopButton()).toBeInTheDocument());
    expect(stub().generateChatTitle).not.toHaveBeenCalled();

    end('r1', 'completed', '2+2 equals 4.');

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
  });

  it('asks before the button replaces a title somebody chose, and replaces it only on a yes', async () => {
    const user = userEvent.setup();
    fixture.conversations = [conversation(1, 'launchd KeepAlive')];
    exchange(1, '2+2?', '2+2 equals 4.');
    await renderPage();
    await settled('2+2 equals 4.');

    await user.click(titleButton());
    await screen.findByText('Replace conversation title?');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByText('Replace conversation title?')).not.toBeInTheDocument());

    await user.click(titleButton());
    expect(stub().generateChatTitle).not.toHaveBeenCalled();
    await user.click(await screen.findByRole('button', { name: 'Replace' }));

    await screen.findByRole('heading', { name: 'Sums' });
    expect(stub().generateChatTitle).toHaveBeenCalledTimes(1);
    expect(stub().updateConversationTitle).toHaveBeenCalledWith(1, 'Sums');
  });
});
