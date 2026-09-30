/**
 * The chat page on a computer joined to another machine: a switch at the
 * rail's foot picks whose chats the list shows, a far chat is read and
 * carried on but not changed, and the margin names the device behind each
 * turn, "You" only for this machine's own.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage } from '../../../src/services/transport';
import type { HubChat } from '../../../src/types/generated/HubChat';
import { chatTransport, conversation, framesThenWait, wrapper, type ChatFixture } from './chatPageHarness';
import { UNREAD_STORAGE_KEY } from '../../../src/components/ConversationListPanel/useConversationActivity';
import type { ModelChoice } from '../../../src/components/ChatMessagesPanel';
import { guiModel } from '../fixtures/model';
import { act } from '@testing-library/react';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

const FAR_CHATS: HubChat[] = [
  { id: 1, title: 'Why the build broke', updated_at: '2026-09-30 09:13:07' },
  { id: 9, title: 'Hub notes', updated_at: '2026-09-29 18:02:41' },
];

/** A far chat's rows: a phone's turn and the reply to it, then the hub's own turn. */
const FAR_ROWS: ChatMessage[] = [
  { id: 40, conversation_id: 1, role: 'user', content: 'Why did the build break?', created_at: '2026-09-30T09:12:31Z', metadata: { device: 'phone-7c2e' } },
  {
    id: 41,
    conversation_id: 1,
    role: 'assistant',
    content: 'A dependency moved.',
    created_at: '2026-09-30T09:13:07Z',
    metadata: { device: 'phone-7c2e', modelName: 'qwen3-8b' },
  },
  { id: 42, conversation_id: 1, role: 'user', content: 'Typed at the hub.', created_at: '2026-09-30T09:14:00Z' },
];

let fixture: ChatFixture;

function farTransport() {
  return {
    ...chatTransport(fixture),
    listFarChats: vi.fn(async () => FAR_CHATS),
    openFarChat: vi.fn(async (id: number) => ({
      conversation: { id, title: 'Why the build broke', model_id: null, system_prompt: 'You are the hub.', created_at: '', updated_at: '' },
      messages: id === 1 ? FAR_ROWS : [],
    })),
    listFarRuns: vi.fn(async () => []),
    addFarTurn: vi.fn(async (_id: number, runId: string) => ({
      id: runId,
      kind: 'agent',
      status: 'queued',
      created_at_ms: 1,
      conversation_id: 1,
      last_seq: 0,
    })),
    readFarRunEvents: (_id: string, _after: number, signal: AbortSignal) => framesThenWait([], signal),
  };
}

function joined() {
  applyRemoteStatus({
    ...IDLE_STATUS,
    connected: {
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: '3ca82708b995',
      path: 'direct',
      away_for_s: null,
    },
  });
}

function renderPage() {
  return render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
}

beforeEach(() => {
  window.localStorage.clear();
  resetRemoteState();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive'), conversation(2, 'Parsing GGUF')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user', content: 'Asked here.', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  };
  transport.current = farTransport();
});
afterEach(() => resetRemoteState());

/** The row an element is in: the turn's grid. */
function rowOf(element: HTMLElement): HTMLElement {
  return element.closest('.grid') as HTMLElement;
}

describe('ChatPage, the far machine’s chats', () => {
  it('offers no switch when this machine is joined to nothing', async () => {
    renderPage();
    await screen.findByRole('listbox', { name: 'Conversations' });
    expect(screen.queryByRole('group', { name: 'Whose chats' })).not.toBeInTheDocument();
    expect(screen.getByText('This machine')).toBeInTheDocument();
  });

  it('switching to the other machine lists its chats, and switching back keeps this machine’s marks', async () => {
    const user = userEvent.setup();
    joined();
    const marks = JSON.stringify({ 2: Date.now() + 60_000 });
    window.localStorage.setItem(UNREAD_STORAGE_KEY, marks);
    renderPage();
    // The list is drawn afresh for each source: ask for it each time.
    const list = () => screen.getByRole('listbox', { name: 'Conversations' });
    await waitFor(() => expect(within(list()).getByText('launchd KeepAlive')).toBeInTheDocument());
    expect(within(list()).getByText('New')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Other machine/ }));

    await waitFor(() => expect(within(list()).getByText('Why the build broke')).toBeInTheDocument());
    expect(within(list()).getByText('Hub notes')).toBeInTheDocument();
    expect(within(list()).queryByText('launchd KeepAlive')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Other machine/ })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.queryByRole('button', { name: 'New chat' })).not.toBeInTheDocument();
    expect(within(list()).queryByRole('button', { name: 'Delete conversation' })).not.toBeInTheDocument();
    expect(window.localStorage.getItem(UNREAD_STORAGE_KEY)).toBe(marks);

    await user.click(screen.getByRole('button', { name: /This machine/ }));

    await waitFor(() => expect(within(list()).getByText('launchd KeepAlive')).toBeInTheDocument());
    await waitFor(() => expect(within(list()).getByText('New')).toBeInTheDocument());
    expect(window.localStorage.getItem(UNREAD_STORAGE_KEY)).toBe(marks);
  });

  it('a far chat names the device behind each turn, and no one there is "You"', async () => {
    const user = userEvent.setup();
    joined();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /Other machine/ }));

    const asked = rowOf(await screen.findByText('Why did the build break?'));
    expect(within(asked).getByText('phone-7c2e')).toBeInTheDocument();
    const reply = rowOf(screen.getByText('A dependency moved.'));
    expect(within(reply).getByText('qwen3-8b')).toBeInTheDocument();
    expect(within(reply).getByText('for phone-7c2e')).toBeInTheDocument();
    const hub = rowOf(screen.getByText('Typed at the hub.'));
    expect(within(hub).getByText('Other machine')).toBeInTheDocument();
    expect(screen.queryByText('You')).not.toBeInTheDocument();
  });

  it('a far chat is read and carried on, not changed', async () => {
    const user = userEvent.setup();
    joined();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /Other machine/ }));
    await screen.findByText('Why did the build break?');

    for (const name of ['Edit message', 'Delete message', 'Regenerate reply']) {
      expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    }
    expect(screen.queryByRole('button', { name: /Rename conversation/ })).not.toBeInTheDocument();
    expect(screen.queryByText('System prompt')).not.toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: /console/i })).not.toBeInTheDocument();
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });

  it('this machine’s own chat still says "You"', async () => {
    joined();
    renderPage();
    const asked = rowOf(await screen.findByText('Asked here.'));
    expect(within(asked).getByText('You')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Edit message' })).toBeInTheDocument();
  });

  it('a turn a paired device added to one of this machine’s chats names the device, not "You"', async () => {
    fixture.rows[2] = [
      { id: 21, conversation_id: 2, role: 'user', content: 'Sent from the phone.', created_at: '2026-09-01T09:12:00Z', metadata: { device: 'phone-7c2e' } },
    ];
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('option', { name: /Parsing GGUF/ }));

    const asked = rowOf(await screen.findByText('Sent from the phone.'));
    expect(within(asked).getByText('phone-7c2e')).toBeInTheDocument();
    expect(within(asked).queryByText('You')).not.toBeInTheDocument();
  });

  it('keeps the switch while a far chat is open and the connection drops, so this machine is one click away', async () => {
    const user = userEvent.setup();
    joined();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /Other machine/ }));
    await screen.findByText('Why did the build break?');

    act(() => applyRemoteStatus({ ...IDLE_STATUS }));

    expect(screen.getByRole('group', { name: 'Whose chats' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Other machine/ })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: /Other machine/ })).toHaveTextContent('not connected');
    await user.click(screen.getByRole('button', { name: /This machine/ }));
    await screen.findByText('Asked here.');
  });

  it('a turn just sent on a far chat is "You" while its reply is written', async () => {
    const user = userEvent.setup();
    joined();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /Other machine/ }));
    await screen.findByText('Why did the build break?');

    await user.type(screen.getByRole('textbox'), 'And how do I fix it?{Enter}');

    const sent = rowOf(await screen.findByText('And how do I fix it?'));
    expect(within(sent).getByText('You')).toBeInTheDocument();
    const far = transport.current as { addFarTurn: ReturnType<typeof vi.fn> };
    expect(far.addFarTurn).toHaveBeenCalledWith(1, expect.stringMatching(/^chat-/), 'And how do I fix it?');
  });

  it('a model switch that lands while a far chat is open opens no conversation here', async () => {
    const user = userEvent.setup();
    joined();
    act(() => ingestServerEvent({ type: 'running', modelId: '8', port: 5555, updatedAt: Date.now(), modelName: 'llama-3.2-3b' }));
    (transport.current as { listModels: unknown }).listModels = vi.fn(async () => [
      guiModel({ id: 7, name: 'qwen3' }),
      guiModel({ id: 8, name: 'llama-3.2-3b' }),
    ]);
    const onSwitchModel = vi.fn(
      async (_choice: ModelChoice, _context: () => { conversationId: number | null; draft: string }) => {},
    );
    render(
      <ChatPage modelName="qwen3" modelId={7} serverPort={4321} onSwitchModel={onSwitchModel} onClose={async () => {}} />,
      { wrapper },
    );
    await screen.findByText('Asked here.');
    await waitFor(() => expect(screen.getByRole('option', { name: 'llama-3.2-3b' })).toBeInTheDocument());
    await user.selectOptions(screen.getByRole('combobox', { name: 'Model' }), 'llama-3.2-3b');
    await waitFor(() => expect(onSwitchModel).toHaveBeenCalledTimes(1));
    const context = onSwitchModel.mock.calls[0][1];
    expect(context().conversationId).toBe(1);

    await user.click(screen.getByRole('button', { name: /Other machine/ }));
    await screen.findByText('Why did the build break?');

    expect(context().conversationId).toBeNull();
    act(() => ingestServerEvent({ type: 'stopped', modelId: '8', port: 5555, updatedAt: Date.now(), modelName: 'llama-3.2-3b' }));
  });
});
