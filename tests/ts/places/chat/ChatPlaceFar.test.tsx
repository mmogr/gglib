/**
 * A chat on a computer joined to another machine: one of that machine's own
 * chats (`/chat/far/:id`, from the sidebar's "On desk" group) is read and
 * carried on but not changed, its model is that machine's pick, and the
 * margin names the device behind each turn, "You" only for this machine's
 * own.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage } from '../../../../src/services/transport';
import type { HubChat } from '../../../../src/types/generated/HubChat';
import { agentRun, chatTransport, conversation, framesThenWait, wrapper, type ChatFixture } from './chatPlaceHarness';
import { farEntry, pairedModels } from '../../fixtures/fakeFarDaemon';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../../src/services/remoteRegistry';
import { resetModelLibrary } from '../../../../src/services/modelRegistry';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';

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
    // Read for a far chat's model; none of them thinks unless a test says so.
    listPairedModels: vi.fn(async () => pairedModels([farEntry('qwen3-8b', 5)])),
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

/** The paired machine as the stored pairing names it, connected or not. */
const PAIRED = {
  ...IDLE_STATUS,
  stored_ticket_fingerprint: '3ca82708b995',
  paired_name: 'desk',
  has_remote_key: true,
};

function joined() {
  applyRemoteStatus({
    ...PAIRED,
    connected: {
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: '3ca82708b995',
      path: 'direct',
      away_for_s: null,
    },
  });
}

/** The far machine's chat 1, as `/chat/far/1` opens it. */
function renderDesk() {
  return render(<ChatPlace desk={{ machineName: 'desk' }} modelName="" conversationId={1} />, { wrapper });
}

function renderPage() {
  return render(<ChatPlace modelName="qwen3" modelId={7} serverPort={4321} conversationId={fixture.conversations[0]?.id ?? null} />, { wrapper });
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
afterEach(() => {
  resetRemoteState();
  resetModelLibrary();
});

/** The row an element is in: the turn's grid. */
function rowOf(element: HTMLElement): HTMLElement {
  return element.closest('.grid') as HTMLElement;
}

describe('ChatPlace, the far machine’s chats', () => {
  it('a far chat names the device behind each turn, and no one there is "You"', async () => {
    joined();
    renderDesk();

    const asked = rowOf(await screen.findByText('Why did the build break?'));
    expect(within(asked).getByText('phone-7c2e')).toBeInTheDocument();
    const reply = rowOf(screen.getByText('A dependency moved.'));
    expect(within(reply).getByText('qwen3-8b')).toBeInTheDocument();
    expect(within(reply).getByText('for phone-7c2e')).toBeInTheDocument();
    const hub = rowOf(screen.getByText('Typed at the hub.'));
    expect(within(hub).getByText('Other machine')).toBeInTheDocument();
    expect(screen.queryByText('You')).not.toBeInTheDocument();
  });

  it('a far chat is read and carried on, not changed: no title ▾, no system prompt line, its machine picks the model', async () => {
    joined();
    renderDesk();
    await screen.findByText('Why did the build break?');

    for (const name of ['Edit message', 'Edit reply', 'Delete message', 'Regenerate reply', 'Branch from here']) {
      expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
    }
    // Its title is plain text, not the menu of what changes a chat; it has a
    // prompt of its own, and still no line says so.
    const title = screen.getByRole('heading', { name: 'Why the build broke' });
    expect(within(title).queryByRole('button')).not.toBeInTheDocument();
    expect(screen.queryByText('Custom system prompt')).not.toBeInTheDocument();
    expect(screen.getByText('desk picks the model')).toBeInTheDocument();
    expect(screen.getByRole('textbox')).toBeInTheDocument();
  });

  it('a far chat offers images even where this machine\'s model reads none, and shows a turn\'s from the far store', async () => {
    const shot = { id: 'd'.repeat(64), mime: 'image/png', width: 640, height: 480 };
    const far = transport.current as Record<string, unknown>;
    far.openFarChat = vi.fn(async (id: number) => ({
      conversation: { id, title: 'Why the build broke', model_id: null, system_prompt: 'You are the hub.', created_at: '', updated_at: '' },
      messages: [{ ...FAR_ROWS[0], images: [shot] }],
    }));
    far.getModel = vi.fn(async () => ({ quantization: 'Q8_0', imageInput: false }));
    far.fetchAttachmentBlob = vi.fn(async () => new Blob(['png'], { type: 'image/png' }));
    const made = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:far-shot');
    joined();
    const local = renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Attach an image' })).toBeDisabled());

    local.unmount();
    renderDesk();

    expect(await screen.findByRole('img', { name: 'Image, 640 × 480' })).toHaveAttribute('src', 'blob:far-shot');
    expect(far.fetchAttachmentBlob).toHaveBeenCalledWith('far', shot.id);
    expect(screen.getByRole('button', { name: 'Attach an image' })).toBeEnabled();
    made.mockRestore();
  });

  /** Open the far chat with these figures on its reply's row, as the far machine saved them. */
  async function openFarWith(figures: Record<string, unknown>) {
    (transport.current as Record<string, unknown>).openFarChat = vi.fn(async (id: number) => ({
      conversation: { id, title: 'Why the build broke', model_id: null, system_prompt: 'You are the hub.', created_at: '', updated_at: '' },
      messages: [FAR_ROWS[0], { ...FAR_ROWS[1], metadata: { ...FAR_ROWS[1].metadata, ...figures } }, FAR_ROWS[2]],
    }));
    joined();
    renderDesk();
    await screen.findByText('A dependency moved.');
  }

  it('a far chat shows the context ring where the far machine saved its context size', async () => {
    await openFarWith({ promptTokens: 8000, completionTokens: 200, contextSize: 32768 });
    const ring = screen.getByRole('button', { name: 'Context: 25 percent of context used' });
    expect(ring).toHaveAttribute('title', '8,200 of 32,768 tokens (25%) after the last finished reply.');
  });

  it('a far chat whose rows carry counts and no size shows no ring', async () => {
    await openFarWith({ promptTokens: 8000, completionTokens: 200 });
    expect(screen.getByText('8,000 tok read')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Context: / })).not.toBeInTheDocument();
  });

  it('a far chat offers no Unload and no picker: its model is the other machine’s', async () => {
    joined();
    renderDesk();
    await screen.findByText('Why did the build break?');

    expect(screen.queryByRole('button', { name: /Unload/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: 'Model' })).not.toBeInTheDocument();
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
    render(<ChatPlace modelName="qwen3" modelId={7} serverPort={4321} conversationId={2} />, { wrapper });

    const asked = rowOf(await screen.findByText('Sent from the phone.'));
    expect(within(asked).getByText('phone-7c2e')).toBeInTheDocument();
    expect(within(asked).queryByText('You')).not.toBeInTheDocument();
  });

  it('a turn just sent on a far chat is "You" while its reply is written', async () => {
    const user = userEvent.setup();
    joined();
    renderDesk();
    await screen.findByText('Why did the build break?');

    await user.type(screen.getByRole('textbox'), 'And how do I fix it?{Enter}');

    const sent = rowOf(await screen.findByText('And how do I fix it?'));
    expect(within(sent).getByText('You')).toBeInTheDocument();
    const far = transport.current as { addFarTurn: ReturnType<typeof vi.fn> };
    // Its text and no image, nothing said of thinking, and no drawing: neither button was touched.
    expect(far.addFarTurn).toHaveBeenCalledWith(1, expect.stringMatching(/^chat-/), 'And how do I fix it?', [], undefined, false, undefined);
  });

  it('a far chat still called New Chat is not titled here when a reply finishes in it', async () => {
    const user = userEvent.setup();
    const far = transport.current as Record<string, unknown>;
    far.listFarChats = vi.fn(async () => [{ ...FAR_CHATS[0], title: 'New Chat' }]);
    // The run ends as soon as it is read, its reply finished.
    far.readFarRunEvents = async function* (id: string) {
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    };
    const generateChatTitle = vi.fn(async () => 'Builds');
    far.generateChatTitle = generateChatTitle;
    joined();
    const desk = renderDesk();
    await screen.findByText('Why did the build break?');
    expect(screen.getByRole('heading', { name: 'New Chat' })).toBeInTheDocument();

    await user.type(screen.getByRole('textbox'), 'And how do I fix it?{Enter}');
    // The run's end reads the far chat again: once at opening, once now.
    await waitFor(() => expect(far.openFarChat).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());

    desk.unmount();
    renderPage();
    await screen.findByText('Asked here.');
    expect(generateChatTitle).not.toHaveBeenCalled();
  });

});
