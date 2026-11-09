/**
 * The composer: one `[+]` menu (Add image…, Use a folder…, Draw a picture,
 * Tools ▸), the chips that are set, the context ring once there is a
 * reading, the model, and Send, which becomes Stop.
 *
 * Tools ▸ lists the tool registry as it stands when the menu opens, built-in
 * tools included without opening MCP settings first (CH-11); for a model
 * that cannot call tools it says so instead. The ring follows the rule every
 * gglib client follows: drawn from the figures the last reply carried, and
 * not at all where its context size is missing.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage, McpTool } from '../../../../src/services/transport';
import {
  agentRun,
  chatTransport,
  conversation,
  plusButton,
  plusItem,
  plusSubmenu,
  wrapper,
  type ChatFixture,
} from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import { ChatPlaceWithList as ChatPlace } from './chatPlaceHarness';
import { getToolRegistry, resetToolRegistry, syncBuiltinTools } from '../../../../src/services/tools';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../../src/services/remoteRegistry';

function savedExchange(): ChatMessage[] {
  return [
    { id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' },
    {
      id: 12,
      conversation_id: 1,
      role: 'assistant',
      content: 'It restarts the job whenever it exits.',
      created_at: '2026-09-01T09:13:00Z',
    },
  ];
}

let fixture: ChatFixture;

function renderLocal(conversationId: number | null = 1) {
  return render(
    <ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onSwitchModel={async () => {}} conversationId={conversationId} />,
    { wrapper },
  );
}

/** The composer: the form its text box is in. */
const composer = () => screen.getByRole('textbox', { name: 'Message' }).closest('form') as HTMLElement;

const tool = (name: string): McpTool => ({ name, description: `What ${name} does`, input_schema: { type: 'object', properties: {} } });

beforeEach(() => {
  window.localStorage.clear();
  resetToolRegistry();
  fixture = { conversations: [conversation(1, 'launchd KeepAlive')], rows: { 1: savedExchange() }, runs: [], frames: {} };
  transport.current = chatTransport(fixture);
});
afterEach(() => resetRemoteState());

describe('ChatPlace, the composer', () => {
  it('names the model in its picker, its quantisation on hover, and never makes one up', async () => {
    const page = renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const picker = within(composer()).getByRole('button', { name: 'Model: Qwen3.8-27B' });
    await waitFor(() => expect(picker).toHaveAttribute('title', 'Qwen3.8-27B · Q8_0'));
    page.unmount();

    (transport.current as { getModel: () => Promise<unknown> }).getModel = async () => ({ quantization: null });
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(within(composer()).getByRole('button', { name: 'Model: Qwen3.8-27B' })).toHaveAttribute('title', 'Qwen3.8-27B');
    expect(composer()).not.toHaveTextContent(/Q\d/);
  });

  it('has one [+] menu: Add image…, Use a folder…, Draw a picture and Tools, each greyed with its reason where it cannot be used', async () => {
    const user = userEvent.setup();
    (transport.current as { getModel: () => Promise<unknown> }).getModel = async () => ({ quantization: 'Q8_0', imageInput: false });
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await user.click(plusButton());

    const menu = screen.getByRole('menu');
    // It holds submenus, which open beside it: a scrolling box would clip them.
    expect(menu).not.toHaveClass('overflow-y-auto');
    expect(Array.from(menu.querySelectorAll('[role^="menuitem"]'), (item) => item.textContent)).toEqual([
      'Add image…',
      'Use a folder…',
      'Draw a picture',
      'Tools',
    ]);
    await waitFor(() => expect(plusItem('Add image…')).toBeDisabled());
    expect(plusItem('Add image…')).toHaveAttribute('title', 'This model cannot read images: it has no projector.');
    await waitFor(() => expect(plusItem('Draw a picture')).toBeDisabled());
    expect(plusItem('Draw a picture')).toHaveAttribute('title', 'No image model is installed; download one first');
    expect(plusItem('Use a folder…')).toBeEnabled();
    expect(plusItem('Use a folder…').getAttribute('title')).toMatch(/^Choose a folder on the machine gglib runs on/);
    expect(plusItem('Tools')).toHaveAttribute('aria-haspopup', 'menu');
  });

  it('greys Use a folder… for a model that cannot call tools, and Tools ▸ says it cannot', async () => {
    const user = userEvent.setup();
    (transport.current as { getServerToolSupport: () => Promise<unknown> }).getServerToolSupport = async () => ({
      supports_tool_calls: false,
      detected_format: null,
    });
    await syncWith([tool('get_current_time')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    await waitFor(() => expect(plusItem('Use a folder…')).toBeDisabled());
    expect(plusItem('Use a folder…')).toHaveAttribute('title', "This model can't call tools, so nothing would read a folder.");
    const tools = await plusSubmenu(user, 'Tools');
    expect(within(tools).getByText("This model can't call tools")).toBeInTheDocument();
    expect(within(tools).queryByRole('menuitemcheckbox')).not.toBeInTheDocument();
    expect(within(tools).queryByRole('menuitem', { name: 'Enable all' })).not.toBeInTheDocument();
    expect(within(tools).getByRole('menuitem', { name: 'Agent limits…' })).toBeInTheDocument();
  });

  it('lists the built-in tools in Tools ▸ without opening MCP settings (CH-11), each ticked on and off, the menu left open', async () => {
    const user = userEvent.setup();
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const tools = await plusSubmenu(user, 'Tools');
    const time = within(tools).getByRole('menuitemcheckbox', { name: 'Get Current Time' });
    expect(time).toHaveAttribute('aria-checked', 'false');
    expect(time).toHaveAttribute('title', 'What get_current_time does');
    expect(within(tools).getByRole('menuitemcheckbox', { name: 'Read File' })).toHaveAttribute('aria-checked', 'false');

    await user.click(time);
    expect(getToolRegistry().isEnabled('get_current_time')).toBe(true);
    expect(within(screen.getByRole('menu', { name: 'Tools' })).getByRole('menuitemcheckbox', { name: 'Get Current Time' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    expect(getToolRegistry().isEnabled('read_file')).toBe(false);
  });

  it('switches every tool on with Enable all and off with Disable all, each greyed where it would do nothing', async () => {
    const user = userEvent.setup();
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const tools = await plusSubmenu(user, 'Tools');
    expect(within(tools).getByRole('menuitem', { name: 'Disable all' })).toBeDisabled();
    await user.click(within(tools).getByRole('menuitem', { name: 'Enable all' }));
    expect(getToolRegistry().getEnabledDefinitions().map((t) => t.function.name)).toEqual(['get_current_time', 'read_file']);
    const open = () => screen.getByRole('menu', { name: 'Tools' });
    expect(within(open()).getByRole('menuitem', { name: 'Enable all' })).toBeDisabled();
    expect(within(open()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['true', 'true']);

    await user.click(within(open()).getByRole('menuitem', { name: 'Disable all' }));
    expect(getToolRegistry().getEnabledDefinitions()).toEqual([]);
  });

  it('reads the registry again each time [+] opens: a tool registered since is listed', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    let tools = await plusSubmenu(user, 'Tools');
    expect(within(tools).getByText('No tools registered')).toBeInTheDocument();
    await user.click(plusButton());

    await syncWith([tool('web_search')]);
    tools = await plusSubmenu(user, 'Tools');
    expect(within(tools).getByRole('menuitemcheckbox', { name: 'Web Search' })).toBeInTheDocument();
  });

  it('keeps the agent limits reachable: Tools ▸ › Agent limits… opens them in a sheet', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(screen.queryByLabelText('Tool timeout (ms)')).not.toBeInTheDocument();

    const tools = await plusSubmenu(user, 'Tools');
    await user.click(within(tools).getByRole('menuitem', { name: 'Agent limits…' }));

    const sheet = await screen.findByRole('dialog', { name: 'Agent limits' });
    expect(within(sheet).getByLabelText('Reasoning budget')).toBeInTheDocument();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('closes [+] on Escape, the focus back on [+]; in a submenu, Escape closes that alone, the focus on its item', async () => {
    const user = userEvent.setup();
    await syncWith([tool('get_current_time')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    await user.click(plusButton());
    expect(screen.getByRole('menu')).toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(plusButton()).toHaveAttribute('aria-expanded', 'false');
    expect(plusButton()).toHaveFocus();

    const tools = await plusSubmenu(user, 'Tools');
    within(tools).getByRole('menuitemcheckbox', { name: 'Get Current Time' }).focus();
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu', { name: 'Tools' })).not.toBeInTheDocument();
    expect(plusButton()).toHaveAttribute('aria-expanded', 'true');
    expect(plusItem('Tools')).toHaveFocus();
    expect(plusItem('Tools')).toHaveAttribute('aria-expanded', 'false');
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(plusButton()).toHaveFocus();
  });

  it('gives the focus back to the picker when a choice closes it', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const picker = screen.getByRole('button', { name: 'Model: Qwen3.8-27B' });
    await user.click(picker);
    await user.click(screen.getByRole('menuitemradio', { name: /^Qwen3\.8-27B · / }));
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(picker).toHaveFocus();
  });

  it('says a greyed item\'s reason under it, as its description, not only on hover', async () => {
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await waitFor(() => expect(plusItem('Draw a picture')).toBeDisabled());
    const reason = 'No image model is installed; download one first';
    expect(plusItem('Draw a picture')).toHaveAccessibleDescription(reason);
    expect(within(screen.getByRole('menu')).getByText(reason).tagName).toBe('P');
    // An item that can be used has no such line.
    expect(plusItem('Use a folder…')).not.toHaveAttribute('aria-describedby');
  });

  it('says "Ask <model> anything" in an empty chat and "Reply…" once it has turns', async () => {
    const page = renderLocal(null);
    const box = await screen.findByRole('textbox', { name: 'Message' });
    expect(box).toHaveAttribute('placeholder', 'Ask Qwen3.8-27B anything');
    page.unmount();

    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(screen.getByRole('textbox', { name: 'Message' })).toHaveAttribute('placeholder', 'Reply…');
  });

  it('asks in a saved chat with no turns, as Start over leaves one: the thread is empty', async () => {
    fixture.rows[1] = [];
    renderLocal();
    const box = await screen.findByRole('textbox', { name: 'Message' });
    await act(async () => {});
    expect(box).toHaveAttribute('placeholder', 'Ask Qwen3.8-27B anything');
  });

  it("makes a new chat's conversation for the model the chat is on, on its first send", async () => {
    const user = userEvent.setup();
    const t = transport.current as Record<string, unknown>;
    t.startAgentRun = vi.fn(async (id: string, turn: { conversation_id: number }) => agentRun(id, turn.conversation_id, 'queued'));
    renderLocal(null);
    await user.type(await screen.findByRole('textbox', { name: 'Message' }), 'Hello{Enter}');
    await waitFor(() => expect(t.startAgentRun).toHaveBeenCalledTimes(1));
    expect(t.createConversation).toHaveBeenCalledWith(expect.objectContaining({ model: { machine: { kind: 'local' }, id: 7 } }));
  });

  it('has Send, in ink, which becomes Stop while a reply is written', async () => {
    const page = renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const send = within(composer()).getByRole('button', { name: 'Send' });
    expect(send).toHaveClass('bg-primary');
    expect(within(composer()).queryByRole('button', { name: 'Stop' })).not.toBeInTheDocument();
    page.unmount();

    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'text_delta', content: 'Still going.' }];
    renderLocal();
    await screen.findByText('Still going.');
    const stop = await within(composer()).findByRole('button', { name: 'Stop' });
    expect(stop).toHaveClass('bg-primary');
    expect(within(composer()).queryByRole('button', { name: 'Send' })).not.toBeInTheDocument();
  });

  it('says the model is loading, and for how long, while the reply waits for it, in place of the picker', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0 }];
    renderLocal();
    const line = await within(document.body).findByText(/^Loading Qwen3\.8-27B… \d+ s$/);
    expect(line).toHaveAttribute('role', 'status');
    expect(screen.queryByRole('button', { name: /^Model: / })).not.toBeInTheDocument();
  });

  it('says nothing of loading while a reply waits behind an image render', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'waiting', reason: 'image_render', step: 3, total: 20, position: 1 }];
    renderLocal();
    await screen.findByText('Queued behind an image render');
    expect(screen.queryByText(/^Loading Qwen3\.8-27B…/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Model: Qwen3.8-27B' })).toBeInTheDocument();
  });
});

describe('ChatPlace, a chat on the paired machine', () => {
  const away = (seconds: number | null) => ({
    ...IDLE_STATUS,
    connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: '3ca82708b995', path: 'direct' as const, away_for_s: seconds },
    paired_name: 'desk',
  });

  function renderPaired(conversationId: number | null = 1) {
    return render(
      <ChatPlace
        paired={{ far: { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 }, machineName: 'desk' }}
        modelName="qwen3"
        conversationId={conversationId}
      />,
      { wrapper },
    );
  }

  it('asks the model by its own name in an empty chat; only the plain picker says "on desk"', async () => {
    act(() => applyRemoteStatus(away(null)));
    renderPaired(null);
    const box = await screen.findByRole('textbox', { name: 'Message' });
    expect(box).toHaveAttribute('placeholder', 'Ask qwen3 anything');
    expect(within(composer()).getByText('qwen3 on desk')).toBeInTheDocument();
  });

  it('says the paired model is loading by its own name while a reply waits for it', async () => {
    act(() => applyRemoteStatus(away(null)));
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0 }];
    renderPaired();
    expect(await screen.findByText(/^Loading qwen3… \d+ s$/)).toHaveAttribute('role', 'status');
    expect(screen.queryByText(/on desk…/)).not.toBeInTheDocument();
  });

  it("makes a new chat's conversation for the paired machine's model, on its first send", async () => {
    const user = userEvent.setup();
    act(() => applyRemoteStatus(away(null)));
    const t = transport.current as Record<string, unknown>;
    t.startAgentRun = vi.fn(async (id: string, turn: { conversation_id: number }) => agentRun(id, turn.conversation_id, 'queued'));
    renderPaired(null);
    await user.type(await screen.findByRole('textbox', { name: 'Message' }), 'Hello{Enter}');
    await waitFor(() => expect(t.startAgentRun).toHaveBeenCalledTimes(1));
    expect(t.createConversation).toHaveBeenCalledWith(
      expect.objectContaining({ model: { machine: { kind: 'paired', fingerprint: '3ca82708b995' }, id: 3 } }),
    );
  });

  const startAgentRun = vi.fn(async () => {});
  it('names the model as plain text, and while desk is away says so and disables Send and Enter', async () => {
    (transport.current as Record<string, unknown>).startAgentRun = startAgentRun;
    act(() => applyRemoteStatus(away(null)));
    renderPaired();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(within(composer()).getByText('qwen3 on desk')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Model: / })).not.toBeInTheDocument();
    await userEvent.setup().type(screen.getByRole('textbox', { name: 'Message' }), 'Still there?');
    expect(within(composer()).getByRole('button', { name: 'Send' })).toBeEnabled();

    act(() => applyRemoteStatus(away(0)));
    const now = new Date();
    const since = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;
    expect(within(composer()).getByText(`desk is away since ${since}. Your message will wait.`)).toBeInTheDocument();
    expect(within(composer()).queryByText('qwen3 on desk')).not.toBeInTheDocument();
    expect(within(composer()).getByRole('button', { name: 'Send' })).toBeDisabled();

    // Nor does Enter send: the message waits in the box.
    await userEvent.setup().type(screen.getByRole('textbox', { name: 'Message' }), '{Enter}');
    await act(async () => {});
    expect(startAgentRun).not.toHaveBeenCalled();
    // Enter, not sending, is a new line in it.
    expect((screen.getByRole('textbox', { name: 'Message' }) as HTMLTextAreaElement).value).toBe('Still there?\n');
  });
});

describe('ChatPlace, the context ring', () => {
  /** The composer's ring, by its name; null when the page draws none. */
  const ring = () => screen.queryByRole('button', { name: /^Context: / });
  const detail = () => screen.queryByRole('group', { name: 'Context' });
  /** The ring's track, the circle under its arc: there whenever a ring is drawn, named or not. */
  const track = () => composer().querySelector('circle.stroke-border');

  /** What the ring draws: its arc, the circle over the track, as long as `used` of `size` round its radius. */
  function expectArc(trigger: HTMLElement, used: number, size: number) {
    const arc = trigger.querySelectorAll('circle')[1];
    const radius = Number(arc.getAttribute('r'));
    expect(radius).toBeGreaterThan(0);
    const percent = Math.min(100, Math.floor((200 * used + size) / (2 * size)));
    expect(Number(arc.getAttribute('stroke-dashoffset'))).toBeCloseTo(2 * Math.PI * radius * (1 - percent / 100), 6);
  }

  /** Give the saved reply these figures, as its row keeps them. */
  function replyMade(figures: Record<string, unknown>) {
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, ...figures };
  }
  const QUARTER = { promptTokens: 8000, completionTokens: 200, contextSize: 32768 };

  function renderPlain() {
    return render(
      <ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={fixture.conversations[0]?.id ?? null} />,
      { wrapper },
    );
  }

  it('draws no ring where the last reply has no figures', async () => {
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(ring()).not.toBeInTheDocument();
    // Nor a ring with no name: an empty one would still draw its track.
    expect(track()).toBeNull();
  });

  it('draws nothing where the last reply has counts and no context size: not a ring, a zero or a dash', async () => {
    replyMade({ promptTokens: 8000, completionTokens: 200 });
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(ring()).not.toBeInTheDocument();
    expect(track()).toBeNull();
    expect(composer()).not.toHaveTextContent(/%|—|\b0\b/);
  });

  it('draws the ring after [+], says "Context: N of M used" on hover and opens the detail on a click', async () => {
    const user = userEvent.setup();
    replyMade(QUARTER);
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = within(composer()).getByRole('button', { name: 'Context: 25 percent of context used' });
    expect(trigger).toHaveAttribute('title', 'Context: 8,200 of 32,768 used');
    // Under 70% the ring stands alone: no figure beside it.
    expect(trigger.textContent).toBe('');
    expect(trigger.querySelectorAll('svg')).toHaveLength(1);
    expect(track()).not.toBeNull();
    expectArc(trigger, 8200, 32768);
    expect(trigger).toHaveClass('text-text-secondary');
    expect(plusButton().compareDocumentPosition(trigger) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    expect(trigger).toHaveAttribute('aria-expanded', 'false');
    expect(detail()).not.toBeInTheDocument();
    await user.click(trigger);
    expect(trigger).toHaveAttribute('aria-expanded', 'true');
    expect(detail()).toHaveTextContent('8,200 of 32,768 tokens (25%) after the last finished reply.');
    expect(trigger).toHaveAttribute('aria-controls', detail()!.id);
    // A group, never a dialog: a dialog would hold every other popout open.
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(detail()).not.toBeInTheDocument();
    await user.click(trigger);
    expect(detail()).toBeInTheDocument();
    await user.click(trigger);
    expect(detail()).not.toBeInTheDocument();
    await user.click(trigger);
    await user.click(screen.getByRole('textbox', { name: 'Message' }));
    expect(detail()).not.toBeInTheDocument();
  });

  it('from 70% says the figure beside the ring, in the warning colour, and the detail says why', async () => {
    const user = userEvent.setup();
    replyMade({ promptTokens: 24000, completionTokens: 400, contextSize: 32768, trimmedMessages: 3, finishReason: 'length' });
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = screen.getByRole('button', { name: 'Context: 74 percent of context used, filling up' });
    expect(trigger.textContent).toBe('74%');
    expect(trigger).toHaveClass('text-warning', 'tabular-nums');
    expect(trigger).toHaveAttribute('title', 'Context: 24,400 of 32,768 used');
    expectArc(trigger, 24400, 32768);

    await user.click(trigger);
    expect(Array.from(detail()!.querySelectorAll('p'), (p) => p.textContent)).toEqual([
      '24,400 of 32,768 tokens (74%) after the last finished reply.',
      'Context is filling up.',
      '3 earlier messages were shortened or left out to fit.',
      'The last reply was cut off before it finished.',
    ]);
  });

  it('from 90% says it is almost full, in the danger colour and in words', async () => {
    replyMade({ promptTokens: 29000, completionTokens: 492, contextSize: 32768 });
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = screen.getByRole('button', { name: 'Context: 90 percent of context used, almost full' });
    expect(within(trigger).getByText('90%')).toBeInTheDocument();
    expect(trigger).toHaveClass('text-danger');
    expect(trigger).not.toHaveClass('text-warning');
    expectArc(trigger, 29492, 32768);
  });

  /** A second exchange after the saved one, its reply's row keeping this metadata. */
  function secondReply(metadata: Record<string, unknown>) {
    fixture.rows[1].push(
      { id: 14, conversation_id: 1, role: 'user', content: 'And when it crashes?', created_at: '2026-09-01T09:14:00Z' },
      { id: 15, conversation_id: 1, role: 'assistant', content: 'It also', created_at: '2026-09-01T09:14:30Z', metadata },
    );
  }

  it('keeps the reading of the reply before one that was stopped', async () => {
    replyMade(QUARTER);
    secondReply({ incomplete: true });
    renderPlain();
    await screen.findByText('It also');
    expect(screen.getByRole('button', { name: 'Context: 25 percent of context used' })).toBeInTheDocument();
  });

  it('borrows no size for a newest reply that has its counts and none: no ring, though the reply before has one', async () => {
    replyMade(QUARTER);
    secondReply({ promptTokens: 9000, completionTokens: 300 });
    renderPlain();
    await screen.findByText('It also');
    expect(ring()).not.toBeInTheDocument();
  });

  it('holds the last reading while a reply arrives without its figures', async () => {
    replyMade(QUARTER);
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'prompt_progress', processed: 1240, total: 3420, cached: 2100, time_ms: 900 }];
    renderPlain();
    await screen.findByText('Reading the prompt');
    expect(screen.getByRole('button', { name: 'Context: 25 percent of context used' })).toBeInTheDocument();
  });

  it("takes a reply's figures when they come, while its run is still live", async () => {
    replyMade(QUARTER);
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [
      { type: 'text_delta', content: 'Still going.' },
      { type: 'turn_usage', prompt_tokens: 29000, completion_tokens: 492, context_size: 32768 },
    ];
    renderPlain();
    await screen.findByText('Still going.');
    // Waited for by its title, which is cheap to look for; the name is read once it is there.
    const trigger = await screen.findByTitle('Context: 29,492 of 32,768 used');
    expect(trigger).toHaveAccessibleName('Context: 90 percent of context used, almost full');
  });

  it('shows each chat its own reading: none for a chat whose reply has no size', async () => {
    const user = userEvent.setup();
    replyMade(QUARTER);
    fixture.conversations.push(conversation(2, 'Parsing GGUF'));
    fixture.rows[2] = [
      { id: 21, conversation_id: 2, role: 'user', content: 'What is a tensor?', created_at: '2026-09-01T10:00:00Z' },
      { id: 22, conversation_id: 2, role: 'assistant', content: 'A block of numbers.', created_at: '2026-09-01T10:00:09Z' },
    ];
    renderPlain();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(ring()).toBeInTheDocument();

    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    await screen.findByText('A block of numbers.');
    expect(ring()).not.toBeInTheDocument();
  });
});

/** The daemon lists `tools` as built-in, and the registry is filled from it as the app does at start. */
async function syncWith(tools: McpTool[]): Promise<void> {
  const current = transport.current as Record<string, unknown>;
  current.listBuiltinTools = vi.fn(async () => tools);
  await syncBuiltinTools();
}
