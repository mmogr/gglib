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
import { resetToolRegistry, syncBuiltinTools } from '../../../../src/services/tools';
import { refreshChats } from '../../../../src/services/chatRegistry';
import { useToastContext } from '../../../../src/contexts/ToastContext';
import type { ConversationSettingsPatch } from '../../../../src/types/generated/ConversationSettingsPatch';
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

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};

/** Tools ▸, open. */
const toolsMenu = () => screen.getByRole('menu', { name: 'Tools' });

type Stub = ReturnType<typeof chatTransport>;
const stub = () => transport.current as Stub;

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
  });

  it('lists the built-in tools in Tools ▸ without opening MCP settings (CH-11), every one ticked on a chat that saved none', async () => {
    const user = userEvent.setup();
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const tools = await plusSubmenu(user, 'Tools');
    const time = within(tools).getByRole('menuitemcheckbox', { name: 'Get Current Time' });
    expect(time).toHaveAttribute('aria-checked', 'true');
    expect(time).toHaveAttribute('title', 'What get_current_time does');
    expect(within(tools).getByRole('menuitemcheckbox', { name: 'Read File' })).toHaveAttribute('aria-checked', 'true');
    expect(within(tools).getByRole('menuitem', { name: 'Enable all' })).toBeDisabled();
  });

  it('saves an untick with the chat, only tools, the menu left open, and shows the chat as it is read again', async () => {
    const user = userEvent.setup();
    const save = stub().updateConversationSettings;
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    await plusSubmenu(user, 'Tools');
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Get Current Time' }));
    expect(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Get Current Time' })).toHaveAttribute('aria-checked', 'false');
    await waitFor(() => expect(save.mock.calls).toEqual([[1, { tools: ['builtin:read_file'] }]]));
    await waitFor(() => expect(fixture.conversations[0].settings).toEqual({ tools: ['builtin:read_file'] }));

    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Get Current Time' }));
    await waitFor(() => expect(save).toHaveBeenLastCalledWith(1, { tools: ['builtin:read_file', 'builtin:get_current_time'] }));
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['true', 'true']);
  });

  it('opens a chat on the tools it saved: one by its name, and none at all where they are off', async () => {
    const user = userEvent.setup();
    fixture.conversations[0] = { ...fixture.conversations[0], settings: { tools: ['builtin:read_file'] } };
    await syncWith([tool('get_current_time'), tool('read_file')]);
    const page = renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['false', 'true']);
    page.unmount();

    fixture.conversations[0] = { ...fixture.conversations[0], settings: { tools: ['builtin:read_file'], no_tools: true } };
    await refreshChats('this');
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['false', 'false']);
    expect(within(toolsMenu()).getByRole('menuitem', { name: 'Disable all' })).toBeDisabled();
  });

  it('saves Disable all as no tools, and Enable all as every tool again', async () => {
    const user = userEvent.setup();
    const save = stub().updateConversationSettings;
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    await plusSubmenu(user, 'Tools');
    await user.click(within(toolsMenu()).getByRole('menuitem', { name: 'Disable all' }));
    await waitFor(() => expect(save.mock.calls).toEqual([[1, { no_tools: true }]]));
    await waitFor(() => expect(within(toolsMenu()).getByRole('menuitem', { name: 'Disable all' })).toBeDisabled());
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['false', 'false']);

    await user.click(within(toolsMenu()).getByRole('menuitem', { name: 'Enable all' }));
    await waitFor(() => expect(save).toHaveBeenLastCalledWith(1, { no_tools: false }));
    await waitFor(() => expect(within(toolsMenu()).getByRole('menuitem', { name: 'Enable all' })).toBeDisabled());
  });

  it('shows the saved tools again, and says why, where gglib refused the change', async () => {
    const user = userEvent.setup();
    stub().updateConversationSettings.mockRejectedValueOnce(new Error('Conversation not found: 1'));
    await syncWith([tool('get_current_time'), tool('read_file')]);
    render(
      <>
        <ToastProbe />
        <ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onSwitchModel={async () => {}} conversationId={1} />
      </>,
      { wrapper },
    );
    await screen.findByText('It restarts the job whenever it exits.');

    await plusSubmenu(user, 'Tools');
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' }));
    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent("Could not save this chat's tools: Conversation not found: 1"));
    await waitFor(() =>
      expect(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' })).toHaveAttribute('aria-checked', 'true'),
    );
  });

  it('holds a new chat\'s choice until a send makes the chat: nothing is saved before', async () => {
    const user = userEvent.setup();
    const save = stub().updateConversationSettings;
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal(null);
    await screen.findByRole('textbox', { name: 'Message' });

    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['true', 'true']);
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' }));
    expect(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' })).toHaveAttribute('aria-checked', 'false');
    expect(save).not.toHaveBeenCalled();
  });

  it('shows a new chat\'s choice from its first send on, and saves a later untick after it, from the choice', async () => {
    const user = userEvent.setup();
    const save = stub().updateConversationSettings;
    let land!: () => void;
    save.mockImplementationOnce(async (id: number, patch: ConversationSettingsPatch) => {
      await new Promise<void>((resolve) => (land = resolve));
      fixture.conversations = fixture.conversations.map((c) => (c.id === id ? { ...c, settings: { tools: patch.tools } } : c));
    });
    (transport.current as Record<string, unknown>).startAgentRun = vi.fn(async () => {});
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal(null);
    await screen.findByRole('textbox', { name: 'Message' });
    await plusSubmenu(user, 'Tools');
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' }));
    await user.keyboard('{Escape}{Escape}');

    await user.type(screen.getByRole('textbox', { name: 'Message' }), 'hello{Enter}');
    await waitFor(() => expect(save.mock.calls).toEqual([[2, { tools: ['builtin:get_current_time'] }]]));
    // The chat is open, its list read before the write landed: the choice is still shown.
    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['true', 'false']);
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' }));
    expect(save).toHaveBeenCalledTimes(1);

    await act(async () => land());
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    expect(save.mock.calls[1]).toEqual([2, { tools: ['builtin:get_current_time', 'builtin:read_file'] }]);
  });

  it('offers Enable all on a chat that saved a list of every tool, which saves every tool to come', async () => {
    const user = userEvent.setup();
    fixture.conversations[0] = { ...fixture.conversations[0], settings: { tools: ['builtin:get_current_time', 'builtin:read_file'] } };
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.getAttribute('aria-checked'))).toEqual(['true', 'true']);
    expect(within(toolsMenu()).getByRole('menuitem', { name: 'Enable all' })).toBeEnabled();
    await user.click(within(toolsMenu()).getByRole('menuitem', { name: 'Enable all' }));
    await waitFor(() => expect(stub().updateConversationSettings.mock.calls).toEqual([[1, { tools: [] }]]));
  });

  it('lists each enabled tool server, one not started as one row for all its tools, and keeps it when a builtin is unticked', async () => {
    const user = userEvent.setup();
    stub().listMcpServers.mockResolvedValue([
      { server: { id: 4, name: 'github', enabled: true }, status: 'stopped', tools: [] },
      { server: { id: 5, name: 'old', enabled: false }, status: 'stopped', tools: [] },
    ]);
    await syncWith([tool('get_current_time'), tool('read_file')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await plusSubmenu(user, 'Tools');
    await waitFor(() =>
      expect(within(toolsMenu()).getAllByRole('menuitemcheckbox').map((item) => item.textContent)).toEqual([
        'Get Current Time',
        'Read File',
        'github · all its tools',
      ]),
    );
    await user.click(within(toolsMenu()).getByRole('menuitemcheckbox', { name: 'Read File' }));
    await waitFor(() => expect(stub().updateConversationSettings.mock.calls).toEqual([[1, { tools: ['builtin:get_current_time', '4:*'] }]]));
  });

  it('offers no checklist until the tool servers are read, and says why where they cannot be', async () => {
    const user = userEvent.setup();
    stub().listMcpServers.mockRejectedValue(new Error('the daemon did not answer'));
    await syncWith([tool('get_current_time')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const tools = await plusSubmenu(user, 'Tools');
    await waitFor(() => expect(within(tools).getByText("Couldn't read the tool servers: the daemon did not answer")).toBeInTheDocument());
    expect(within(tools).queryByRole('menuitemcheckbox')).not.toBeInTheDocument();
  });

  it('offers no Agent limits… : they are Settings › Tools\' now, for every chat', async () => {
    const user = userEvent.setup();
    await syncWith([tool('get_current_time')]);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    await plusSubmenu(user, 'Tools');
    expect(within(toolsMenu()).queryByRole('menuitem', { name: 'Agent limits…' })).not.toBeInTheDocument();
    expect(screen.queryByRole('dialog', { name: 'Agent limits' })).not.toBeInTheDocument();
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

  it('says the model is loading in the reply, in the daemon\'s words, and keeps the picker and the text box', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [
      { type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0, model: 'Qwen3.8-27B', elapsed_ms: 7_200, said: 'Loading Qwen3.8-27B… 7 s' },
    ];
    renderLocal();
    const line = await screen.findByText('Loading Qwen3.8-27B… 7 s');
    expect(line.closest('[role="status"]')?.closest('[data-turn="reply"]')).not.toBeNull();
    expect(within(composer()).queryByText(/Loading/)).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Model: Qwen3.8-27B' })).toBeInTheDocument();
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeEnabled();
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

  it('says the paired model is loading in the words its wait came with', async () => {
    act(() => applyRemoteStatus(away(null)));
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'waiting', reason: 'model_load', step: 0, total: 0, position: 0, model: 'qwen3', elapsed_ms: 3_000, said: 'Loading qwen3… 3 s' }];
    renderPaired();
    expect(await screen.findByText('Loading qwen3… 3 s')).toBeInTheDocument();
    expect(screen.queryByText(/on desk…/)).not.toBeInTheDocument();
    expect(within(composer()).getByText('qwen3 on desk')).toBeInTheDocument();
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
