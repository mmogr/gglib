/**
 * The chat's title ▾ and its system prompt line. The title (16/600) is the
 * menu button: Rename · Generate title · System prompt… · Export · Start
 * over (same prompt)… — Delete…. A custom system prompt is one tertiary line,
 * "Custom system prompt · Edit", above the first turn; the default shows
 * nothing; the editor is inline (Reset to default, Cancel, Save). An empty
 * chat has nothing to change yet: its title is plain.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { chatTransport, conversation, fromTitleMenu, wrapper, type ChatFixture } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as Record<string, unknown> }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';
import { DEFAULT_SYSTEM_PROMPT } from '../../../../src/hooks/useGglibRuntime';

let fixture: ChatFixture;
const onOpened = vi.fn();
const onDeleted = vi.fn();

function renderPlace(conversationId: number | null = 1) {
  return render(
    <ChatPlace modelName="qwen3" modelId={7} serverPort={4321} conversationId={conversationId} onOpened={onOpened} onDeleted={onDeleted} />,
    { wrapper },
  );
}

/** The stub's call `name`, as a mock. */
const call = (name: string) => transport.current[name] as ReturnType<typeof vi.fn>;

/**
 * The title's menu button, once the chat's turns are drawn: the head is drawn
 * again with them, and a button found before then is gone.
 */
async function titleButton(title = 'launchd KeepAlive') {
  await screen.findByText('What does KeepAlive do?');
  return within(await screen.findByRole('heading', { name: title })).getByRole('button');
}

beforeEach(() => {
  onOpened.mockClear();
  onDeleted.mockClear();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive')],
    rows: { 1: [{ id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' }] },
    runs: [],
    frames: {},
  };
  transport.current = {
    ...chatTransport(fixture),
    updateConversationTitle: vi.fn(async (id: number, title: string) => {
      fixture.conversations = fixture.conversations.map((c) => (c.id === id ? { ...c, title } : c));
    }),
    updateConversationSystemPrompt: vi.fn(async (id: number, prompt: string | null) => {
      fixture.conversations = fixture.conversations.map((c) => (c.id === id ? { ...c, system_prompt: prompt } : c));
    }),
  };
});
afterEach(() => vi.restoreAllMocks());

describe("ChatPlace, the title ▾", () => {
  it('is the title itself, and holds what changes the chat, Delete… set apart last', async () => {
    const user = userEvent.setup();
    renderPlace();
    const button = await titleButton();
    expect(button).toHaveAttribute('aria-haspopup', 'menu');
    expect(button.querySelector('.text-base.font-semibold')).toHaveTextContent('launchd KeepAlive');

    await user.click(button);
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual([
      'Rename',
      'Generate title',
      'System prompt…',
      'Export',
      'Start over (same prompt)…',
      'Delete…',
    ]);
    expect(screen.getByRole('menuitem', { name: 'Delete…' })).toHaveClass('text-danger', 'mt-sm');
  });

  it('Rename puts a field in the title, and Enter saves it', async () => {
    const user = userEvent.setup();
    renderPlace();
    await titleButton();
    await fromTitleMenu(user, 'Rename');
    const field = screen.getByRole('textbox', { name: 'Conversation title' });
    await user.clear(field);
    await user.type(field, 'Daemons{Enter}');

    await waitFor(() => expect(call('updateConversationTitle')).toHaveBeenCalledWith(1, 'Daemons'));
    expect(await screen.findByRole('heading', { name: 'Daemons' })).toBeInTheDocument();
  });

  it('Generate title is offered but greyed, saying why, while nothing serves the chat here', async () => {
    const user = userEvent.setup();
    render(<ChatPlace desk={undefined} paired={{ far: { machine: { kind: 'paired', fingerprint: 'ab' }, id: 3 }, machineName: 'desk' }} modelName="qwen3" conversationId={1} />, { wrapper });
    await user.click(await titleButton());
    const item = screen.getByRole('menuitem', { name: 'Generate title' });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute('title', 'Start a server to generate titles');
  });

  it('Export reads the saved thread and hands it over as a file', async () => {
    const user = userEvent.setup();
    const made = vi.spyOn(URL, 'createObjectURL').mockImplementation(() => 'blob:export');
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    const clicked = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});
    renderPlace();
    await titleButton();
    await fromTitleMenu(user, 'Export');

    await waitFor(() => expect(clicked).toHaveBeenCalledTimes(1));
    expect(call('getThread')).toHaveBeenCalledWith(1);
    expect(made).toHaveBeenCalledTimes(1);
  });

  it('Start over asks, then puts an empty copy with the same title and prompt in its place, and opens it', async () => {
    const user = userEvent.setup();
    fixture.conversations = [{ ...conversation(1, 'launchd KeepAlive'), system_prompt: 'Answer in haiku.' }];
    renderPlace();
    await titleButton();
    await fromTitleMenu(user, 'Start over (same prompt)…');
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(call('deleteConversation')).not.toHaveBeenCalled();

    await fromTitleMenu(user, 'Start over (same prompt)…');
    await user.click(await screen.findByRole('button', { name: 'Start over' }));

    await waitFor(() => expect(onOpened).toHaveBeenCalledTimes(1));
    expect(call('deleteConversation')).toHaveBeenCalledWith(1);
    // The copy keeps this machine's model the chat is on, so its next turn runs there.
    expect(call('createConversation')).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'launchd KeepAlive',
        systemPrompt: 'Answer in haiku.',
        model: { machine: { kind: 'local' }, id: 7 },
      }),
    );
    // The copy the daemon made is the one opened.
    expect(onOpened).toHaveBeenCalledWith(await call('createConversation').mock.results[0].value);
  });

  it('Delete… asks, deletes only on a yes, and says the open chat is gone', async () => {
    const user = userEvent.setup();
    renderPlace();
    await titleButton();
    await fromTitleMenu(user, 'Delete…');
    expect(await screen.findByText('Delete "launchd KeepAlive"?')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(call('deleteConversation')).not.toHaveBeenCalled();
    expect(onDeleted).not.toHaveBeenCalled();

    await fromTitleMenu(user, 'Delete…');
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(onDeleted).toHaveBeenCalledTimes(1));
    expect(call('deleteConversation')).toHaveBeenCalledWith(1);
  });

  it('an empty chat, before its first send, has a plain title and no prompt line', async () => {
    renderPlace(null);
    const title = await screen.findByRole('heading', { name: 'New Chat' });
    expect(within(title).queryByRole('button')).not.toBeInTheDocument();
    expect(screen.queryByText('Custom system prompt')).not.toBeInTheDocument();
  });
});

describe('ChatPlace, the system prompt line', () => {
  it('shows nothing for the default prompt; System prompt… opens the editor above the first turn, and Save makes the line', async () => {
    const user = userEvent.setup();
    renderPlace();
    await screen.findByText('What does KeepAlive do?');
    expect(screen.queryByText('Custom system prompt')).not.toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'System prompt' })).not.toBeInTheDocument();

    await fromTitleMenu(user, 'System prompt…');
    const editor = screen.getByRole('textbox', { name: 'System prompt' });
    expect(editor).toHaveValue(DEFAULT_SYSTEM_PROMPT);
    // Above the first turn.
    expect(editor.compareDocumentPosition(screen.getByText('What does KeepAlive do?')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    await user.clear(editor);
    await user.type(editor, 'Answer in haiku.');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(call('updateConversationSystemPrompt')).toHaveBeenCalledWith(1, 'Answer in haiku.'));
    expect(await screen.findByText('Custom system prompt')).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: 'System prompt' })).not.toBeInTheDocument();
  });

  it('a draft left by Cancel is gone when the editor opens again: it starts from what the chat has', async () => {
    const user = userEvent.setup();
    renderPlace();
    await screen.findByText('What does KeepAlive do?');
    await fromTitleMenu(user, 'System prompt…');
    await user.type(screen.getByRole('textbox', { name: 'System prompt' }), ' Never mind.');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    await fromTitleMenu(user, 'System prompt…');
    expect(screen.getByRole('textbox', { name: 'System prompt' })).toHaveValue(DEFAULT_SYSTEM_PROMPT);
  });

  it("a custom prompt is one line; Edit opens it, Cancel keeps it, and Reset to default saves the chat back to none", async () => {
    const user = userEvent.setup();
    fixture.conversations = [{ ...conversation(1, 'launchd KeepAlive'), system_prompt: 'Answer in haiku.' }];
    renderPlace();
    await screen.findByText('What does KeepAlive do?');
    const line = (await screen.findByText('Custom system prompt')).parentElement as HTMLElement;
    expect(line).toHaveClass('text-xs', 'text-text-muted');
    const edit = within(line).getByRole('button', { name: 'Edit' });
    expect(edit).toHaveAttribute('title', 'Answer in haiku.');

    await user.click(edit);
    expect(screen.getByRole('textbox', { name: 'System prompt' })).toHaveValue('Answer in haiku.');
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('textbox', { name: 'System prompt' })).not.toBeInTheDocument();
    expect(call('updateConversationSystemPrompt')).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Edit' }));
    await user.click(screen.getByRole('button', { name: 'Reset to default' }));
    expect(screen.getByRole('textbox', { name: 'System prompt' })).toHaveValue(DEFAULT_SYSTEM_PROMPT);
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(call('updateConversationSystemPrompt')).toHaveBeenCalledWith(1, null));
    await waitFor(() => expect(screen.queryByText('Custom system prompt')).not.toBeInTheDocument());
  });
});
