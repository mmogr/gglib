/**
 * The chat page's Thinking switch, on each kind of chat: a button in the
 * composer's margin, there only where the chat's model thinks, pressed while
 * the chat thinks and not where gglib remembers it switched off, when it
 * reads "Thinking off"; a click changes what the next send says, once, and
 * what the Tools popout sets for the whole device goes with every send as it
 * did. A choice whose turn was accepted is over: a chat left while its reply
 * is written, and changed since on another device, opens as that device left
 * it.
 *
 * The daemon here remembers as the real one does: a run that says `off`
 * leaves its conversation saying so and one that says `default` leaves it
 * without, in a conversation that is replaced and never changed in place, so
 * the page learns of it only by reading again.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage, ConversationSettings } from '../../../src/services/transport';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { HubChat } from '../../../src/types/generated/HubChat';
import type { ModelInfo } from '../../../src/types/generated/ModelInfo';
import type { ModelRef } from '../../../src/types/generated/ModelRef';
import type { Thinking } from '../../../src/types/generated/Thinking';
import { agentRun, chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';
import { FAR_FINGERPRINT, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';
import { guiModel } from '../fixtures/model';
import { CAPABILITY_FLAGS, type GgufModel } from '../../../src/types';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

const OVERRIDES_KEY = 'gglib.chat.agentOverrides';
const ON_TITLE = 'Thinking is on for this chat. Click to switch it off from the next message.';
const OFF_TITLE = 'Thinking is off for this chat. Click to switch it back on from the next message.';

let fixture: ChatFixture;
/** The body of each run the page started, in order. */
let started: AgentRunRequest[];
/** What the far machine remembers of its chat 1, and what it lists. */
let farSettings: ConversationSettings | undefined;
let farRows: ChatMessage[];
let farModels: ModelInfo[];
let nextRow: number;

type Stub = ReturnType<typeof pageTransport>;
const stub = () => transport.current as Stub;

/** The settings of a chat after a run that said `thinking`, as the daemon writes them. */
function remembering(settings: ConversationSettings | null | undefined, thinking: Thinking): ConversationSettings {
  const next: ConversationSettings = { ...settings };
  delete next.thinking;
  if (thinking === 'off') next.thinking = 'off';
  return next;
}

/** A page on this machine's model `model`: a whole catalogue entry, as the daemon sends one. */
function pageTransport(model: Partial<GgufModel> = { tags: ['reasoning'] }) {
  return {
    ...chatTransport(fixture),
    getModel: vi.fn(async (): Promise<GgufModel> => guiModel({ id: 7, quantization: 'Q8_0', ...model })),
    startAgentRun: vi.fn(async (id: string, request: AgentRunRequest) => {
      started.push(request);
      const cid = request.conversation_id as number;
      const said = request.thinking;
      if (said) {
        fixture.conversations = fixture.conversations.map((c) =>
          c.id === cid ? { ...c, settings: remembering(c.settings, said) } : c,
        );
      }
      const asked = String(request.messages.at(-1)?.content ?? '');
      fixture.rows[cid] = [
        ...(fixture.rows[cid] ?? []),
        { id: nextRow++, conversation_id: cid, role: 'user', content: asked, created_at: '2026-09-01T10:00:00Z' },
      ];
      return agentRun(id, cid, 'queued');
    }),
    // A run that ends as soon as it is read: its reply is whatever was saved.
    readRunEvents: async function* (id: string, _after: number, _signal: AbortSignal) {
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    },
    listPairedModels: vi.fn(async () => pairedModels(farModels)),
    listFarChats: vi.fn(async (): Promise<HubChat[]> => [{ id: 1, title: 'Why the build broke', updated_at: '2026-09-30 09:13:07' }]),
    listFarRuns: vi.fn(async () => []),
    openFarChat: vi.fn(async (id: number) => ({
      conversation: {
        id,
        title: 'Why the build broke',
        model_id: null,
        system_prompt: 'You are the hub.',
        created_at: '',
        updated_at: '',
        ...(farSettings && { settings: { ...farSettings } }),
      },
      messages: [...farRows],
    })),
    addFarTurn: vi.fn(async (id: number, runId: string, content: string, _images: string[], said?: Thinking) => {
      if (said) farSettings = remembering(farSettings, said);
      farRows = [...farRows, { id: nextRow++, conversation_id: id, role: 'user', content, created_at: '2026-09-30T09:20:00Z' }];
      return agentRun(runId, id, 'queued');
    }),
    readFarRunEvents: async function* (id: string, _after: number, _signal: AbortSignal) {
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    },
  };
}

function connect() {
  applyRemoteStatus({
    ...IDLE_STATUS,
    stored_ticket_fingerprint: FAR_FINGERPRINT,
    paired_name: 'desk',
    has_remote_key: true,
    connected: {
      port: 41234,
      base_url: 'http://127.0.0.1:41234/v1',
      ticket_fingerprint: FAR_FINGERPRINT,
      path: 'direct',
      away_for_s: null,
    },
  });
}

const farRef: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };

function renderLocal() {
  return render(<ChatPage modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
}

function renderPaired() {
  return render(
    <ChatPage paired={{ far: farRef, machineName: 'desk' }} modelName="qwen3" onClose={async () => {}} />,
    { wrapper },
  );
}

/** Everything `fn` was asked so far has been answered, and the page has taken the answers. */
async function answered(fn: { mock: { results: Array<{ value: unknown }> } }) {
  await act(async () => {
    await Promise.all(fn.mock.results.map((r) => r.value));
  });
}

const composer = () => screen.getByRole('textbox', { name: 'Message' });
/** The row an element is in: the turn's grid. */
const rowOf = (element: HTMLElement) => element.closest('.grid') as HTMLElement;
/** The switch, in the composer's row; null when the page draws none. */
const chip = () => within(rowOf(composer())).queryByRole('button', { name: 'Thinking' });

/** Send `text` from the composer, and wait for its run to start and end and for this machine's list to be read after it. */
async function exchange(user: ReturnType<typeof userEvent.setup>, text: string): Promise<AgentRunRequest> {
  const runs = started.length;
  const reads = stub().listConversations.mock.calls.length;
  await user.type(composer(), `${text}{Enter}`);
  await waitFor(() => expect(started).toHaveLength(runs + 1));
  await waitFor(() => expect(stub().listConversations.mock.calls.length).toBeGreaterThan(reads));
  await answered(stub().listConversations);
  await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
  return started[runs];
}

beforeEach(() => {
  window.localStorage.clear();
  resetRemoteState();
  started = [];
  nextRow = 100;
  farSettings = { model_name: 'qwen3-8b' };
  farRows = [
    { id: 40, conversation_id: 1, role: 'user', content: 'Why did the build break?', created_at: '2026-09-30T09:12:31Z' },
    { id: 41, conversation_id: 1, role: 'assistant', content: 'A dependency moved.', created_at: '2026-09-30T09:13:07Z', metadata: { modelName: 'qwen3-8b' } },
  ];
  farModels = [farEntry('qwen3', 3, { capabilities: ['reasoning'] }), farEntry('qwen3-8b', 5, { capabilities: ['reasoning'] })];
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive'), conversation(2, 'Parsing GGUF')],
    rows: {
      1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }],
      2: [{ id: 21, conversation_id: 2, role: 'user', content: 'What is a tensor?', created_at: '2026-09-01T10:00:00Z' }],
    },
    runs: [],
    frames: {},
  };
  transport.current = pageTransport();
});
afterEach(() => resetRemoteState());

describe("ChatPage, the Thinking switch on this machine's model", () => {
  it('is a pressed button named Thinking, after the context ring, that says which way it is and takes the keyboard', async () => {
    const user = userEvent.setup();
    fixture.rows[1].push({
      id: 12, conversation_id: 1, role: 'assistant', content: 'It restarts the job.', created_at: '2026-09-01T09:13:00Z',
      metadata: { promptTokens: 8000, completionTokens: 200, contextSize: 32768 },
    });
    renderLocal();
    await screen.findByText('It restarts the job.');

    const button = await within(rowOf(composer())).findByRole('button', { name: 'Thinking' });
    expect(button.tagName).toBe('BUTTON');
    expect(button).toBeEnabled();
    expect(button).toHaveAttribute('aria-pressed', 'true');
    expect(button).toHaveAttribute('title', ON_TITLE);
    const ring = screen.getByRole('button', { name: /^Context: / });
    expect(ring.compareDocumentPosition(button) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The ring and the tools button beside it are as they were.
    expect(ring).toHaveAttribute('title', '8,200 of 32,768 tokens (25%) after the last finished reply.');
    expect(within(rowOf(composer())).getByRole('button', { name: 'Tools' })).toBeInTheDocument();

    button.focus();
    expect(button).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(chip()).toHaveAttribute('title', OFF_TITLE);
    await user.keyboard(' ');
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
  });

  it('reads "Thinking" while on and "Thinking off" while off, beside another icon, under the one name', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());

    const on = chip()!;
    expect(on).toHaveAttribute('aria-pressed', 'true');
    expect(on.textContent).toBe('Thinking');
    const brain = on.querySelector('svg')!.outerHTML;

    // Switched off it is the same button, named Thinking and not pressed:
    // its words and its icon say which way it is, not its colours alone.
    await user.click(on);
    const off = chip()!;
    expect(off).toHaveAccessibleName('Thinking');
    expect(off).toHaveAttribute('aria-pressed', 'false');
    expect(off.textContent).toBe('Thinking off');
    expect(off.querySelector('svg')!.outerHTML).not.toBe(brain);

    await user.click(off);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect(chip()!.textContent).toBe('Thinking');
    expect(chip()!.querySelector('svg')!.outerHTML).toBe(brain);
  });

  it.each([
    ['has no reasoning tag', { tags: ['agent'] }],
    ['has only the template\'s reasoning bit', { tags: [], capabilities: CAPABILITY_FLAGS.supportsReasoning }],
  ])('draws none for a model that %s', async (_, model) => {
    transport.current = pageTransport(model);
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    // The model's entry has landed: its quantisation is drawn from the same answer.
    await within(rowOf(composer())).findByText('Q8_0');
    await answered(stub().getModel);
    expect(chip()).not.toBeInTheDocument();
  });

  it('draws none for a model this machine could not describe', async () => {
    const page = pageTransport();
    page.getModel = vi.fn(async () => {
      throw new Error('the catalogue did not answer');
    });
    transport.current = page;
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(page.getModel).toHaveBeenCalled());
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(chip()).not.toBeInTheDocument();
  });

  it('opens not pressed on a chat that remembers Off, and its send says nothing: gglib already knows', async () => {
    const user = userEvent.setup();
    fixture.conversations[0] = { ...fixture.conversations[0], settings: { thinking: 'off' } };
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
    expect(chip()).toHaveAttribute('title', OFF_TITLE);

    expect(Object.keys(await exchange(user, 'one'))).not.toContain('thinking');
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
  });

  it('says off on the send after a click and never again, then default once after a click back, with the device-wide effort and budget on every send', async () => {
    const user = userEvent.setup();
    window.localStorage.setItem(OVERRIDES_KEY, JSON.stringify({ reasoningEffort: 'high', reasoningBudgetTokens: 2048 }));
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());
    // The device-wide budget says nothing of this chat: the switch shows on.
    expect(chip()).toHaveAttribute('aria-pressed', 'true');

    expect(Object.keys(await exchange(user, 'one'))).not.toContain('thinking');

    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect((await exchange(user, 'two')).thinking).toBe('off');
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(Object.keys(await exchange(user, 'three'))).not.toContain('thinking');
    expect(chip()).toHaveAttribute('aria-pressed', 'false');

    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect((await exchange(user, 'four')).thinking).toBe('default');
    expect(Object.keys(await exchange(user, 'five'))).not.toContain('thinking');
    expect(chip()).toHaveAttribute('aria-pressed', 'true');

    expect(started).toHaveLength(5);
    for (const request of started) {
      expect(request).toMatchObject({ conversation_id: 1, reasoning_effort: 'high', reasoning_budget_tokens: 2048 });
    }
  });

  it('a device-wide budget of 0 neither switches a chat off nor is replaced by the switch', async () => {
    const user = userEvent.setup();
    window.localStorage.setItem(OVERRIDES_KEY, JSON.stringify({ reasoningBudgetTokens: 0 }));
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'true'));

    const sent = await exchange(user, 'one');
    expect(sent.reasoning_budget_tokens).toBe(0);
    expect(Object.keys(sent)).not.toContain('thinking');
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
  });

  it('a send gglib refused keeps the choice: the next send says it', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());
    await user.click(chip()!);

    const accept = stub().startAgentRun.getMockImplementation()!;
    stub().startAgentRun.mockImplementationOnce(async () => {
      throw new Error('the model is not ready');
    });
    await user.type(composer(), 'one{Enter}');
    await screen.findByText('the model is not ready');
    expect(started).toHaveLength(0);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    // The text came back to the composer; send it again.
    await waitFor(() => expect(composer()).toHaveValue('one'));
    expect(stub().startAgentRun.getMockImplementation()).toBe(accept);

    expect((await exchange(user, '{Enter}')).thinking).toBe('off');
  });

  /** Hold every run open until the test ends it, as a reply being written. */
  function holdRuns(): () => void {
    let end: () => void = () => {};
    const going = new Promise<void>((resolve) => {
      end = resolve;
    });
    stub().readRunEvents = async function* (id: string, _after: number, _signal: AbortSignal) {
      await going;
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    };
    return end;
  }

  /** Send `text` and wait for its run to be going, its reply not yet ended. */
  async function sendHeld(user: ReturnType<typeof userEvent.setup>, text: string) {
    const runs = started.length;
    await user.type(composer(), `${text}{Enter}`);
    await waitFor(() => expect(started).toHaveLength(runs + 1));
    await screen.findByRole('button', { name: 'Stop' });
    return started[runs];
  }

  /** End the held run, and wait for this machine's list to be read after it. */
  async function endHeld(end: () => void) {
    const reads = stub().listConversations.mock.calls.length;
    await act(async () => end());
    await waitFor(() => expect(stub().listConversations.mock.calls.length).toBeGreaterThan(reads));
    await answered(stub().listConversations);
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
  }

  it('a click while a reply is written is said by the next send, not lost', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());
    const end = holdRuns();

    expect(Object.keys(await sendHeld(user, 'one'))).not.toContain('thinking');
    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    await endHeld(end);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');

    expect((await exchange(user, 'two')).thinking).toBe('off');
  });

  it('a click back during the reply that said off is kept: the next send says default', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());
    const end = holdRuns();

    await user.click(chip()!);
    expect((await sendHeld(user, 'one')).thinking).toBe('off');
    // gglib remembers Off from that send; the page has not read its list since.
    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    await endHeld(end);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');

    expect((await exchange(user, 'two')).thinking).toBe('default');
    expect(Object.keys(await exchange(user, 'three'))).not.toContain('thinking');
  });

  it("keeps each chat's switch its own, and a choice not yet sent through a look at another chat", async () => {
    const user = userEvent.setup();
    fixture.conversations[1] = { ...fixture.conversations[1], settings: { thinking: 'off' } };
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'true'));
    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');

    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    await screen.findByText('What is a tensor?');
    // Chat 2 remembers Off of its own; chat 1's click is not what shows here.
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');

    await user.click(screen.getByRole('option', { name: /launchd KeepAlive/ }));
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
    expect(await exchange(user, 'one')).toMatchObject({ conversation_id: 1, thinking: 'off' });

    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    await screen.findByText('What is a tensor?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'true'));
    expect(await exchange(user, 'two')).toMatchObject({ conversation_id: 2, thinking: 'default' });
  });

  it('shows off on a chat switched off from another device, when it is opened', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'true'));

    // A phone's turn switched chat 2 off since this page read its list.
    fixture.conversations = fixture.conversations.map((c) => (c.id === 2 ? { ...c, settings: { thinking: 'off' } } : c));
    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    await screen.findByText('What is a tensor?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
    expect(Object.keys(await exchange(user, 'one'))).not.toContain('thinking');
  });

  it('opening a chat selects it at once and reads the list again, once and quietly: the list stays drawn', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());

    // The next reading of the list answers only when the test lets it.
    let answer: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      answer = resolve;
    });
    const list = stub().listConversations.getMockImplementation()!;
    const before = stub().listConversations.mock.calls.length;
    stub().listConversations.mockImplementationOnce(async () => {
      await held;
      return list();
    });

    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    // The reading is out and unanswered.
    expect(stub().listConversations.mock.calls.length).toBe(before + 1);
    // The chat is shown without waiting for it.
    await screen.findByText('What is a tensor?');
    // And the list is drawn meanwhile, not replaced while it loads.
    expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /launchd KeepAlive/ })).toBeInTheDocument();

    await act(async () => answer());
    await answered(stub().listConversations);
    expect(stub().listConversations.mock.calls.length).toBe(before + 1);
    expect(screen.getByRole('option', { name: /Parsing GGUF/ })).toBeInTheDocument();
  });

  it("a chat switched on here, then left for the other machine's chats while its reply is written, opens as a phone has since left it", async () => {
    const user = userEvent.setup();
    connect();
    fixture.conversations[0] = { ...fixture.conversations[0], settings: { thinking: 'off' } };
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
    const end = holdRuns();

    // Switched on here, and gglib accepts the run that says so.
    await user.click(chip()!);
    expect((await sendHeld(user, 'one')).thinking).toBe('default');
    await answered(stub().startAgentRun);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect(fixture.conversations[0].settings?.thinking).toBeUndefined();

    // Left for the other machine's chats: this machine's list is not read when the reply ends.
    await user.click(screen.getByRole('button', { name: /desk's chats/ }));
    await screen.findByText('Why did the build break?');
    await act(async () => end());

    // A phone's turn switches the chat off, and this machine's chats are shown again.
    fixture.conversations = fixture.conversations.map((c) => (c.id === 1 ? { ...c, settings: { thinking: 'off' } } : c));
    await user.click(screen.getByRole('button', { name: /This machine/ }));
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));

    expect(Object.keys(await exchange(user, 'two'))).not.toContain('thinking');
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(fixture.conversations.find((c) => c.id === 1)?.settings?.thinking).toBe('off');
  });

  it("asks for the model's template support when the tools popout first opens, and shows a note in place of the effort dropdown where the template reads none", async () => {
    const user = userEvent.setup();
    const page = pageTransport();
    page.getModelDetail = vi.fn(async () => ({ reasoningEffortSupport: 'no' }));
    transport.current = page;
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toBeInTheDocument());
    expect(page.getModelDetail).not.toHaveBeenCalled();

    const tools = within(rowOf(composer())).getByRole('button', { name: 'Tools' });
    await user.click(tools);
    expect(page.getModelDetail).toHaveBeenCalledTimes(1);
    expect(page.getModelDetail).toHaveBeenCalledWith(7);
    expect(await screen.findByText(/template does not declare reasoning effort/)).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: 'Reasoning Effort' })).not.toBeInTheDocument();
    expect(screen.getByLabelText('Reasoning budget')).toBeInTheDocument();

    await user.click(tools);
    await user.click(tools);
    expect(screen.getByText(/template does not declare reasoning effort/)).toBeInTheDocument();
    expect(page.getModelDetail).toHaveBeenCalledTimes(1);
  });
});

describe("ChatPage, the Thinking switch on the paired machine's model", () => {
  it("is shown by the row that machine lists for the model, and its send says off beside the model's ref", async () => {
    const user = userEvent.setup();
    connect();
    renderPaired();
    await screen.findByText('What does KeepAlive do?');
    const button = await within(rowOf(composer())).findByRole('button', { name: 'Thinking' });
    expect(button).toHaveAttribute('aria-pressed', 'true');
    // Nothing of this machine's catalogue was asked for it.
    expect(stub().getModel).not.toHaveBeenCalled();

    await user.click(button);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(await exchange(user, 'one')).toMatchObject({ far: farRef, thinking: 'off' });
    const next = await exchange(user, 'two');
    expect(next).toMatchObject({ far: farRef });
    expect(Object.keys(next)).not.toContain('thinking');
  });

  it('opens not pressed on a chat that remembers Off', async () => {
    connect();
    fixture.conversations[0] = { ...fixture.conversations[0], settings: { thinking: 'off' } };
    renderPaired();
    await screen.findByText('What does KeepAlive do?');
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'false'));
  });

  it('draws none where that machine lists the model without the capability, as a gglib from before the switch does', async () => {
    connect();
    farModels = [farEntry('qwen3', 3, { capabilities: ['embeddings'] }), farEntry('other', 4, { capabilities: ['reasoning'] })];
    renderPaired();
    await screen.findByText('What does KeepAlive do?');
    // The rows have landed: the attach button, offered until they do, is read
    // from the same list and withdrawn for a model that reads no images.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Attach an image' })).toBeDisabled());
    await answered(stub().listPairedModels);
    expect(chip()).not.toBeInTheDocument();
  });
});

describe('ChatPage, the Thinking switch on a far chat', () => {
  /** Open the far machine's chat 1, as the rail's switch does. */
  async function openFar(user: ReturnType<typeof userEvent.setup>) {
    connect();
    renderLocal();
    await user.click(await screen.findByRole('button', { name: /desk's chats/ }));
    await screen.findByText('Why did the build break?');
    await waitFor(() => expect(stub().listPairedModels).toHaveBeenCalled());
    await answered(stub().listPairedModels);
  }

  /** Send `text` on the far chat and wait for its run to end and the chat to be read again. */
  async function farExchange(user: ReturnType<typeof userEvent.setup>, text: string) {
    const turns = stub().addFarTurn.mock.calls.length;
    const opens = stub().openFarChat.mock.calls.length;
    await user.type(composer(), `${text}{Enter}`);
    await waitFor(() => expect(stub().addFarTurn).toHaveBeenCalledTimes(turns + 1));
    await waitFor(() => expect(stub().openFarChat.mock.calls.length).toBeGreaterThan(opens));
    await answered(stub().openFarChat);
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument());
    return stub().addFarTurn.mock.calls[turns];
  }

  it("is shown by the row its machine lists for the model the chat last ran on, though this machine's model does not think", async () => {
    const user = userEvent.setup();
    transport.current = pageTransport({ tags: [] });
    await openFar(user);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect(chip()).toHaveAttribute('title', ON_TITLE);
  });

  it('an untouched turn says nothing of thinking; a click adds off to the next turn alone', async () => {
    const user = userEvent.setup();
    await openFar(user);

    expect(await farExchange(user, 'one')).toEqual([1, expect.stringMatching(/^chat-/), 'one', [], undefined, false]);
    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(await farExchange(user, 'two')).toEqual([1, expect.stringMatching(/^chat-/), 'two', [], 'off', false]);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(await farExchange(user, 'three')).toEqual([1, expect.stringMatching(/^chat-/), 'three', [], undefined, false]);
    // No run was started on this machine for any of them.
    expect(started).toEqual([]);
  });

  it('opens not pressed on a chat that machine remembers as Off, and a click back says default once', async () => {
    const user = userEvent.setup();
    farSettings = { model_name: 'qwen3-8b', thinking: 'off' };
    await openFar(user);
    expect(chip()).toHaveAttribute('aria-pressed', 'false');
    expect(chip()).toHaveAttribute('title', OFF_TITLE);

    await user.click(chip()!);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect((await farExchange(user, 'one'))[4]).toBe('default');
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
    expect((await farExchange(user, 'two'))[4]).toBeUndefined();
  });

  /**
   * That machine with a second chat beside chat 1, each remembering its own
   * settings, and replies that stay being written until `end` is called or
   * their reader leaves.
   */
  function twoFarChats(first: ConversationSettings) {
    const settings: Record<number, ConversationSettings> = { 1: first, 2: { model_name: 'qwen3-8b' } };
    const titles: Record<number, string> = { 1: 'Why the build broke', 2: 'Second far chat' };
    stub().listFarChats.mockImplementation(async () => [
      { id: 1, title: titles[1], updated_at: '2026-09-30 09:13:07' },
      { id: 2, title: titles[2], updated_at: '2026-09-30 09:10:00' },
    ]);
    stub().openFarChat.mockImplementation(async (id: number) => ({
      conversation: { id, title: titles[id], model_id: null, system_prompt: 'You are the hub.', created_at: '', updated_at: '', settings: { ...settings[id] } },
      messages:
        id === 1
          ? [...farRows]
          : [{ id: 60, conversation_id: 2, role: 'user', content: 'Second chat question', created_at: '2026-09-30T09:00:00Z' }],
    }));
    stub().addFarTurn.mockImplementation(async (id: number, runId: string, content: string, _images: string[], said?: Thinking) => {
      if (said) settings[id] = remembering(settings[id], said);
      farRows = [...farRows, { id: nextRow++, conversation_id: id, role: 'user', content, created_at: '2026-09-30T09:20:00Z' }];
      return agentRun(runId, id, 'queued');
    });
    let end: () => void = () => {};
    const going = new Promise<void>((resolve) => {
      end = resolve;
    });
    stub().readFarRunEvents = async function* (id: string, _after: number, signal: AbortSignal) {
      await Promise.race([going, new Promise<void>((resolve) => signal.addEventListener('abort', () => resolve()))]);
      yield { type: 'end' as const, info: agentRun(id, 1, 'completed') };
    };
    return { settings, end };
  }

  it.each([
    ['on', 'default', 'off', 'false', 'true'],
    ['off', 'off', undefined, 'true', 'false'],
  ] as const)(
    'a chat switched %s here and left while its reply is written opens as a phone has since left it, and the next turn says nothing',
    async (_, said, remembers, pressed, switched) => {
      const user = userEvent.setup();
      const first: ConversationSettings = { model_name: 'qwen3-8b', ...(remembers && { thinking: remembers }) };
      const there = twoFarChats({ ...first });
      await openFar(user);
      await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', pressed));

      // Switched here, and that machine accepts the turn that says so.
      await user.click(chip()!);
      expect(chip()).toHaveAttribute('aria-pressed', switched);
      await user.type(composer(), 'one{Enter}');
      await waitFor(() => expect(stub().addFarTurn).toHaveBeenCalledTimes(1));
      expect(stub().addFarTurn.mock.calls[0][4]).toBe(said);
      await answered(stub().addFarTurn);
      expect(there.settings[1].thinking).toBe(said === 'off' ? 'off' : undefined);
      await screen.findByRole('button', { name: 'Stop' });
      // It still shows while the reply is written, with the chat not read since.
      expect(chip()).toHaveAttribute('aria-pressed', switched);

      // Left while the reply is written: the chat is not read when it ends.
      const reads = () => stub().openFarChat.mock.calls.filter((call) => call[0] === 1).length;
      const before = reads();
      await user.click(screen.getByRole('option', { name: /Second far chat/ }));
      await screen.findByText('Second chat question');
      await act(async () => there.end());
      expect(reads()).toBe(before);

      // A phone's turn switches it back there, and it is opened again here.
      there.settings[1] = { ...first };
      await user.click(screen.getByRole('option', { name: /Why the build broke/ }));
      await screen.findByText('Why did the build break?');
      await waitFor(() => expect(reads()).toBe(before + 1));
      await answered(stub().openFarChat);
      await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', pressed));

      // The next turn, the switch untouched, says nothing: what the phone chose stands.
      expect((await farExchange(user, 'two'))[4]).toBeUndefined();
      expect(chip()).toHaveAttribute('aria-pressed', pressed);
      expect(there.settings[1].thinking).toBe(remembers);
    },
  );

  it("is shown by the last reply's model for a chat whose settings name none", async () => {
    const user = userEvent.setup();
    farSettings = undefined;
    await openFar(user);
    expect(chip()).toHaveAttribute('aria-pressed', 'true');
  });

  it('draws none for a far chat that has never run, and one appears with its first reply', async () => {
    const user = userEvent.setup();
    farSettings = undefined;
    farRows = [farRows[0]];
    await openFar(user);
    expect(chip()).not.toBeInTheDocument();

    // Its first turn runs there: that machine now says which model it ran on.
    stub().addFarTurn.mockImplementationOnce(async (id: number, runId: string, content: string) => {
      farSettings = { model_name: 'qwen3-8b' };
      farRows = [...farRows, { id: nextRow++, conversation_id: id, role: 'user', content, created_at: '2026-09-30T09:20:00Z' }];
      return agentRun(runId, id, 'queued');
    });
    expect((await farExchange(user, 'one'))[4]).toBeUndefined();
    await waitFor(() => expect(chip()).toHaveAttribute('aria-pressed', 'true'));
  });

  it('draws none where that machine lists no model as thinking, and its turns say nothing of it', async () => {
    const user = userEvent.setup();
    farSettings = { model_name: 'qwen3-8b', thinking: 'off' };
    farModels = [farEntry('qwen3-8b', 5, { capabilities: ['vision'] })];
    await openFar(user);
    expect(chip()).not.toBeInTheDocument();
    expect((await farExchange(user, 'one'))[4]).toBeUndefined();
  });

  it('asks this machine nothing about a template for the tools popout, which says where the effort level applies', async () => {
    const user = userEvent.setup();
    await openFar(user);
    await user.click(within(rowOf(composer())).getByRole('button', { name: 'Tools' }));
    expect(screen.getByRole('combobox', { name: 'Reasoning Effort' })).toBeInTheDocument();
    expect(screen.getByText('Applies to models whose template declares reasoning effort; others ignore it.')).toBeInTheDocument();
    expect(stub().getModelDetail).not.toHaveBeenCalled();
  });
});
