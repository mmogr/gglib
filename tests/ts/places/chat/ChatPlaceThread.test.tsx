/**
 * The chat page's thread: a reading column of turns, a question a filled
 * block and a reply plain text, with no margin. A reply folds its thinking
 * and tool calls into one line that opens in place; its figures are one
 * line, always on the newest reply and on hover elsewhere, with the full
 * set in its tooltip; its model is named only where it changes. Each turn
 * has an action line, always on the newest reply and on hover or focus
 * elsewhere, and its time on hover. A reply arriving says what it waits for
 * until its first token.
 *
 * The rule under test is "only true figures": a figure the page does not
 * have is left out, never drawn as zero or a dash.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { useToastContext } from '../../../../src/contexts/ToastContext';
import type { ChatMessage } from '../../../../src/services/transport';
import { agentRun, chatTransport, ChatPlaceWithList, conversation, wrapper as pageWrapper, type ChatFixture } from './chatPlaceHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPlace from '../../../../src/places/chat/ChatPlace';
import { TransportError } from '../../../../src/services/transport';
import { getToolRegistry } from '../../../../src/services/tools/registry';
import { mcpGenericRenderer } from '../../../../src/services/tools/renderers';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) => pageWrapper({ children: <><ToastProbe />{children}</> });

const ANSWER = 'It restarts the job whenever it exits.';

function savedExchange(): ChatMessage[] {
  return [
    { id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' },
    {
      id: 12,
      conversation_id: 1,
      role: 'assistant',
      content: ANSWER,
      created_at: '2026-09-01T09:13:00Z',
      metadata: {
        thinking: 'The user asks about launchd.',
        thinkingDurationSeconds: 19,
        tool_calls: [{ id: 'c1', name: 'read_file', arguments: { path: 'job.plist' } }],
      },
    },
    { id: 13, conversation_id: 1, role: 'tool', content: '<plist/>', created_at: '2026-09-01T09:13:00Z', metadata: { tool_call_id: 'c1' } },
  ];
}

/** How a reply was made, as its row keeps it. */
const MADE = {
  modelName: 'Qwen3.8-27B',
  modelQuantization: 'Q8_0',
  promptTokens: 3180,
  cachedTokens: 2100,
  completionTokens: 496,
  turnDurationMs: 41_000,
  writingDurationMs: 40_992,
};

let fixture: ChatFixture;
type Stub = ReturnType<typeof chatTransport> & { deleteMessage: ReturnType<typeof vi.fn> };
const stub = () => transport.current as Stub;

function renderLocal() {
  return render(<ChatPlace modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={1} />, { wrapper });
}

/** The turn an element is in. */
const turnOf = (element: HTMLElement) => element.closest('[data-turn]') as HTMLElement;
/** Whether the action line an action is on is shown. */
const lineShown = (action: HTMLElement) => action.closest('[data-shown]')?.getAttribute('data-shown');

/** A second exchange after the saved one, its reply's row keeping `metadata`. */
function secondExchange(metadata: Record<string, unknown> = {}) {
  fixture.rows[1].push(
    { id: 14, conversation_id: 1, role: 'user', content: 'And when it crashes?', created_at: '2026-09-01T09:14:00Z' },
    { id: 15, conversation_id: 1, role: 'assistant', content: 'It starts again.', created_at: '2026-09-01T09:14:30Z', metadata },
  );
}

beforeEach(() => {
  window.localStorage.clear();
  fixture = { conversations: [conversation(1, 'launchd KeepAlive')], rows: { 1: savedExchange() }, runs: [], frames: {} };
  transport.current = { ...chatTransport(fixture), deleteMessage: vi.fn(async () => 1) };
});

describe('ChatPlace, the thread', () => {
  it('lays the turns in one reading column: a question a filled block, a reply plain text, no margin', async () => {
    renderLocal();
    const reply = turnOf(await screen.findByText(ANSWER));
    const question = turnOf(screen.getByText('What does KeepAlive do?'));

    const column = reply.parentElement as HTMLElement;
    expect(column).toHaveClass('max-w-[680px]', 'gap-xl');
    expect(column).toContainElement(question);
    expect(within(question).getByText('What does KeepAlive do?').closest('.bg-surface-elevated')).toHaveClass('text-base', 'text-text');
    expect(screen.getByText(ANSWER).closest('.text-base')).not.toHaveClass('bg-surface-elevated');
    // No who: neither "You" nor "Assistant" heads a turn.
    expect(screen.queryByText('You')).not.toBeInTheDocument();
    expect(screen.queryByText('Assistant')).not.toBeInTheDocument();
  });

  it('folds a reply\'s thinking and tool call into one line that opens in place, with no dialog', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText(ANSWER);

    const fold = screen.getByRole('button', { name: 'Thought for 19 s · 1 tool call' });
    expect(fold).toHaveAttribute('aria-expanded', 'false');
    const body = document.getElementById(fold.getAttribute('aria-controls')!)!;
    expect(body).not.toBeVisible();

    await user.click(fold);
    expect(fold).toHaveAttribute('aria-expanded', 'true');
    expect(within(body).getByText('The user asks about launchd.')).toBeVisible();

    const call = within(body).getByRole('button', { name: 'Read File' });
    const detail = document.getElementById(call.getAttribute('aria-controls')!)!;
    expect(detail).not.toBeVisible();
    await user.click(call);
    expect(call).toHaveAttribute('aria-expanded', 'true');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    expect(detail).toBeVisible();
    expect(within(detail).getByText(/"path": "job\.plist"/)).toBeVisible();
    expect(within(detail).getByText('Result')).toBeVisible();
    expect(detail).toHaveTextContent('<plist/>');

    await user.click(within(detail).getByRole('button', { name: 'Copy result' }));
    expect(await navigator.clipboard.readText()).toBe('"<plist/>"');
    expect(within(detail).getByRole('button', { name: 'Copy result' })).toHaveTextContent('Copied');
  });

  it('names a call\'s MCP server, and says a reply that did not think by its tool calls alone, in the plural', async () => {
    fixture.rows[1][1].metadata = {
      tool_calls: [
        { id: 'c1', name: '3:tavily_search', arguments: {}, display_name: 'Tavily Search', server: 'Tavily Web Search' },
        { id: 'c2', name: 'builtin:get_current_time', arguments: {}, display_name: 'Get Current Time' },
      ],
    };
    fixture.rows[1][2] = { ...fixture.rows[1][2], metadata: { tool_call_id: 'c1' } };
    fixture.rows[1].push({ id: 16, conversation_id: 1, role: 'tool', content: '12:00', created_at: '2026-09-01T09:13:00Z', metadata: { tool_call_id: 'c2' } });
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText(ANSWER);

    await user.click(screen.getByRole('button', { name: '2 tool calls' }));
    const [search, time] = within(screen.getByRole('list', { name: 'Tool calls' })).getAllByRole('listitem');
    const call = within(search).getByRole('button', { name: /^Tavily Search/ });
    expect(call).toHaveTextContent(/^Tavily Searchon Tavily Web Search$/);
    // The name the model called it by, in the call's detail.
    expect(document.getElementById(call.getAttribute('aria-controls')!)).toHaveTextContent('3:tavily_search');
    expect(within(time).getByRole('button', { name: 'Get Current Time' })).toHaveTextContent(/^Get Current Time$/);
  });

  it('shows a reply of only a tool call, or only reasoning, by its fold', async () => {
    fixture.rows[1] = [
      savedExchange()[0],
      { ...savedExchange()[1], content: '', metadata: { tool_calls: [{ id: 'c1', name: 'read_file', arguments: {} }] } },
      savedExchange()[2],
      { id: 14, conversation_id: 1, role: 'assistant', content: '', created_at: '2026-09-01T09:14:00Z', metadata: { thinking: 'Only thought.', thinkingDurationSeconds: 2 } },
    ];
    renderLocal();
    expect(await screen.findByRole('button', { name: '1 tool call' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Thought for 2 s' })).toBeInTheDocument();
    expect(screen.queryByText('No text.')).not.toBeInTheDocument();
  });

  it('keeps the fold live and open while the reply thinks', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'reasoning_delta', content: 'Looking at the plist.' }];
    renderLocal();

    const fold = await screen.findByRole('button', { name: /^Thinking…/ });
    expect(fold).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Looking at the plist.')).toBeVisible();
  });

  it('shows a reply arriving as how far its prompt has been read, with a bar', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'prompt_progress', processed: 1240, total: 3420, cached: 2100, time_ms: 900 }];
    renderLocal();

    const status = (await screen.findByText('Reading the prompt')).closest('[role="status"]') as HTMLElement;
    expect(within(status).getByText('1,240 of 3,420 tokens read')).toBeInTheDocument();
    expect(within(status).getByText('2,100 from cache')).toBeInTheDocument();
    expect(within(status).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '36');
  });

  it('says nothing of the prompt before its first reading, and the line goes with the first token', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    renderLocal();
    const status = (await screen.findByText('Waiting for the model')).closest('[role="status"]') as HTMLElement;
    expect(status).not.toHaveTextContent(/tokens|from cache/);
  });

  it('takes the arrival line away when the first token comes', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [
      { type: 'prompt_progress', processed: 3420, total: 3420, cached: 0, time_ms: 900 },
      { type: 'text_delta', content: 'Still going.' },
    ];
    renderLocal();
    await screen.findByText('Still going.');
    expect(screen.queryByText('Starting the reply')).not.toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });
});

describe('ChatPlace, a reply\'s tool calls', () => {
  type Item = { type: 'frame'; seq: number; data: string } | { type: 'end'; info: ReturnType<typeof agentRun> };
  let feed: (item: Item) => void = () => {};
  let seq = 0;
  /** The run in chat 1 is read as the test feeds it, until it ends. */
  function fedRun() {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    (stub() as { readRunEvents: unknown }).readRunEvents = async function* (_id: string, _after: number, signal: AbortSignal) {
      const queue: Item[] = [];
      let wake = () => {};
      feed = (item) => {
        queue.push(item);
        wake();
      };
      signal.addEventListener('abort', () => wake());
      while (!signal.aborted) {
        while (queue.length > 0) {
          const item = queue.shift()!;
          yield item;
          if (item.type === 'end') return;
        }
        await new Promise<void>((resolve) => (wake = resolve));
      }
    };
  }
  const emit = (event: object) => feed({ type: 'frame', seq: ++seq, data: JSON.stringify(event) });
  const start = (id: string, args: object = {}) =>
    emit({ type: 'tool_call_start', tool_call: { id, name: 'read_file', arguments: args }, display_name: 'Read File' });
  const settle = (id: string, success: boolean, content: string) =>
    emit({
      type: 'tool_call_complete',
      tool_name: 'read_file',
      result: { tool_call_id: id, content, success },
      wait_ms: 0,
      execute_duration_ms: 1200,
      display_name: 'Read File',
      duration_display: '1.2s',
    });

  beforeEach(() => {
    fixture.rows[1] = [savedExchange()[0]];
    seq = 0;
  });

  it('keeps the person\'s choice of the fold after the reply finishes', async () => {
    const user = userEvent.setup();
    fedRun();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    emit({ type: 'reasoning_delta', content: 'Looking at the plist.' });

    const fold = await screen.findByRole('button', { name: /^Thinking…/ });
    expect(fold).toHaveAttribute('aria-expanded', 'true');
    await user.click(fold);
    await user.click(fold);
    expect(fold).toHaveAttribute('aria-expanded', 'true');

    emit({ type: 'text_delta', content: ANSWER });
    fixture.rows[1].push({ ...savedExchange()[1], metadata: { thinking: 'Looking at the plist.', thinkingDurationSeconds: 3 } });
    feed({ type: 'end', info: agentRun('r1', 1, 'completed') });

    // Closed by default once it has arrived; open, as the person left it.
    const done = await screen.findByRole('button', { name: 'Thought for 3 s' });
    expect(done).toHaveAttribute('aria-expanded', 'true');
  });

  it('says a tool that failed, and why, in the danger colour, and tells a screen reader as each call settles', async () => {
    fedRun();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    start('c1');
    start('c2');
    const calls = await screen.findByRole('list', { name: 'Tool calls' });
    const live = () => document.querySelector('[aria-live="polite"]');
    await waitFor(() => expect(live()).toBeInTheDocument());
    expect(live()).toHaveTextContent(/^$/);

    settle('c1', true, '<plist/>');
    await waitFor(() => expect(live()).toHaveTextContent(/^Read File done\.$/));
    const first = live()!.firstElementChild;
    settle('c2', false, 'permission denied');

    const failed = await within(calls).findByText('failed · permission denied');
    expect(failed).toHaveClass('text-danger');
    expect(within(calls).getAllByRole('img', { name: 'Failed' })).toHaveLength(1);
    expect(within(calls).getByText('1.2 s')).not.toHaveClass('text-danger');
    expect(live()).toHaveTextContent(/^Read File done\. Read File failed\.$/);
    // Additions only: the first call's sentence is the same node, not said again.
    expect(live()).toHaveAttribute('aria-relevant', 'additions');
    expect(live()!.children).toHaveLength(2);
    expect(live()!.firstElementChild).toBe(first);
  });

  it('drops the screen reader line once the reply has arrived', async () => {
    fedRun();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    start('c1');
    settle('c1', true, '<plist/>');
    const live = () => document.querySelector('[aria-live="polite"]');
    await waitFor(() => expect(live()).toHaveTextContent('Read File done.'));

    emit({ type: 'text_delta', content: ANSWER });
    fixture.rows[1].push(savedExchange()[1]);
    feed({ type: 'end', info: agentRun('r1', 1, 'completed') });
    await waitFor(() => expect(live()).not.toBeInTheDocument());
  });

  it('copies a call\'s arguments, also while it runs', async () => {
    const user = userEvent.setup();
    fedRun();
    renderLocal();
    await screen.findByText('What does KeepAlive do?');
    start('c1', { path: 'job.plist' });

    const call = await screen.findByRole('button', { name: /Read File/ });
    await user.click(call);
    const detail = document.getElementById(call.getAttribute('aria-controls')!)!;
    expect(within(detail).queryByRole('button', { name: 'Copy result' })).not.toBeInTheDocument();
    await user.click(within(detail).getByRole('button', { name: 'Copy arguments' }));
    expect(await navigator.clipboard.readText()).toBe('{\n  "path": "job.plist"\n}');
  });

  it('draws a result through the tool\'s registered renderer: the generic one for an MCP tool (CH-12)', async () => {
    const registry = getToolRegistry();
    registry.registerWithNameMapping(
      'notes',
      '3',
      'notes',
      { type: 'function', function: { name: 'notes', description: 'Notes', parameters: { type: 'object', properties: {} } } },
      'mcp:3',
      mcpGenericRenderer,
    );
    try {
      fixture.rows[1] = [
        savedExchange()[0],
        { ...savedExchange()[1], metadata: { tool_calls: [{ id: 'c1', name: 'notes', arguments: {} }] } },
        { ...savedExchange()[2], content: '# Release notes\n\n- **fast** paths everywhere' },
      ];
      const user = userEvent.setup();
      renderLocal();
      await user.click(await screen.findByRole('button', { name: '1 tool call' }));
      const call = screen.getByRole('button', { name: /^Notes/ });
      await user.click(call);
      const detail = document.getElementById(call.getAttribute('aria-controls')!)!;
      expect(within(detail).getByRole('heading', { name: 'Release notes' })).toBeVisible();
      expect(within(detail).getByText('fast').tagName).toBe('STRONG');
    } finally {
      registry.unregister('notes');
    }
  });
});

describe('ChatPlace, a reply\'s figures', () => {
  it('shows the newest reply\'s figure line always, with every figure in its tooltip', async () => {
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, ...MADE };
    renderLocal();
    await screen.findByText(ANSWER);

    const line = screen.getByText('41 s · 12 tok/s · 2,100 tokens reused');
    expect(line.getAttribute('title')!.split('\n')).toEqual([
      'Qwen3.8-27B · Q8_0',
      'thought for 19 s',
      '1 tool call',
      '3,180 tokens read',
      '2,100 tokens reused',
      '496 tokens written',
      '41 s · 12 tok/s',
    ]);
  });

  it('shows an earlier reply\'s figure line on hover only', async () => {
    const user = userEvent.setup();
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, ...MADE };
    secondExchange();
    renderLocal();
    const earlier = turnOf(await screen.findByText(ANSWER));
    await screen.findByText('It starts again.');

    expect(within(earlier).queryByText(/tokens reused/)).not.toBeInTheDocument();
    await user.hover(earlier);
    expect(within(earlier).getByText('41 s · 12 tok/s · 2,100 tokens reused')).toBeInTheDocument();
    await user.unhover(earlier);
    expect(within(earlier).queryByText(/tokens reused/)).not.toBeInTheDocument();
  });

  it('says "unfinished" in the warning colour', async () => {
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, ...MADE, incomplete: true };
    renderLocal();
    await screen.findByText(ANSWER);
    const word = screen.getByText('unfinished');
    expect(word).toHaveClass('text-warning');
    expect(word.parentElement).toHaveTextContent('unfinished · 41 s · 12 tok/s · 2,100 tokens reused');
  });

  it('says no rate for a reply of one token', async () => {
    fixture.rows[1][1].metadata = { completionTokens: 1, writingDurationMs: 0, turnDurationMs: 300 };
    renderLocal();
    await screen.findByText(ANSWER);
    expect(screen.getByText('0.3 s')).toBeInTheDocument();
    expect(screen.queryByText(/tok\/s/)).not.toBeInTheDocument();
  });

  it('names a reply\'s model only where it is not the model of the reply before', async () => {
    fixture.rows[1][1].metadata = { modelName: 'qwen3' };
    secondExchange({ modelName: 'qwen3' });
    fixture.rows[1].push(
      { id: 17, conversation_id: 1, role: 'user', content: 'Ask another model.', created_at: '2026-09-01T09:15:00Z' },
      { id: 18, conversation_id: 1, role: 'assistant', content: 'Another view.', created_at: '2026-09-01T09:15:30Z', metadata: { modelName: 'llama-3.2-3b' } },
    );
    renderLocal();
    const last = turnOf(await screen.findByText('Another view.'));

    expect(within(last).getByText('llama-3.2-3b')).toBeInTheDocument();
    expect(screen.queryByText('qwen3')).not.toBeInTheDocument();
  });
});

describe('ChatPlace, the action line', () => {
  it('offers a reply Copy · Retry · Edit · ⋯ (Branch from here), always on the newest reply', async () => {
    const user = userEvent.setup();
    renderLocal();
    const reply = turnOf(await screen.findByText(ANSWER));

    const copy = within(reply).getByRole('button', { name: 'Copy reply' });
    expect(copy).toHaveTextContent('Copy');
    expect(within(reply).getByRole('button', { name: 'Retry this reply' })).toHaveTextContent('Retry');
    expect(within(reply).getByRole('button', { name: 'Edit reply' })).toHaveTextContent('Edit');
    expect(lineShown(copy)).toBe('true');

    await user.click(within(reply).getByRole('button', { name: 'More for this reply' }));
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual(['Branch from here']);
  });

  it('shows an earlier reply\'s line and a question\'s on hover, and on keyboard focus', async () => {
    const user = userEvent.setup();
    secondExchange();
    renderLocal();
    const earlier = turnOf(await screen.findByText(ANSWER));
    const question = turnOf(screen.getByText('What does KeepAlive do?'));
    const reply = within(earlier).getByRole('button', { name: 'Copy reply' });
    const asked = within(question).getByRole('button', { name: 'Copy message' });
    expect(within(question).getByRole('button', { name: 'Edit message' })).toHaveTextContent('Edit');

    expect(lineShown(reply)).toBe('false');
    expect(lineShown(asked)).toBe('false');
    await user.hover(earlier);
    expect(lineShown(reply)).toBe('true');
    await user.unhover(earlier);
    expect(lineShown(reply)).toBe('false');

    within(question).getByRole('button', { name: 'Edit message' }).focus();
    await waitFor(() => expect(lineShown(asked)).toBe('true'));
  });

  it('shows a turn\'s time on hover', async () => {
    const user = userEvent.setup();
    renderLocal();
    const question = turnOf(await screen.findByText('What does KeepAlive do?'));

    expect(question.querySelector('time')).toBeNull();
    await user.hover(question);
    expect(question.querySelector('time')).toHaveAttribute('dateTime', '2026-09-01T09:12:00.000Z');
  });

  it('deletes a question and what follows only once the confirm sheet is answered', async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText(ANSWER);

    await user.click(screen.getByRole('button', { name: 'More for this message' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    const sheet = await screen.findByRole('dialog');
    expect(within(sheet).getByText('Delete this message?')).toBeInTheDocument();
    expect(within(sheet).getByText('The 1 message after it is deleted too. This cannot be undone.')).toBeInTheDocument();
    expect(stub().deleteMessage).not.toHaveBeenCalled();

    await user.click(within(sheet).getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(stub().deleteMessage).toHaveBeenCalledWith(11));
  });

  it('leaving the chat while Delete… is asked takes the question with it, and deletes nothing', async () => {
    const user = userEvent.setup();
    fixture.conversations.push(conversation(2, 'Parsing GGUF'));
    fixture.rows[2] = [
      { id: 21, conversation_id: 2, role: 'user', content: 'What is a tensor?', created_at: '2026-09-01T10:00:00Z' },
      { id: 22, conversation_id: 2, role: 'assistant', content: 'A block of numbers.', created_at: '2026-09-01T10:00:09Z' },
    ];
    render(<ChatPlaceWithList modelName="Qwen3.8-27B" modelId={7} serverPort={4321} conversationId={1} />, { wrapper });
    await screen.findByText(ANSWER);

    await user.click(screen.getByRole('button', { name: 'More for this message' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();

    // Another chat opens under the sheet, as ⌘N or the menu's Show Chat opens one.
    fireEvent.click(screen.getByRole('option', { name: /Parsing GGUF/, hidden: true }));
    await screen.findByText('A block of numbers.');

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(stub().deleteMessage).not.toHaveBeenCalled();
    expect(screen.getByText('A block of numbers.')).toBeInTheDocument();
  });

  it('says why a delete is refused while a reply is being written', async () => {
    const user = userEvent.setup();
    stub().deleteMessage.mockRejectedValueOnce(
      new TransportError('CONFLICT', 'conversation 1 already has a live reply, run e1; stop it or wait'),
    );
    renderLocal();
    await screen.findByText(ANSWER);

    await user.click(screen.getByRole('button', { name: 'More for this message' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Delete' }));

    await waitFor(() =>
      expect(screen.getByTestId('toasts')).toHaveTextContent('A reply is still being written here. Stop it or wait, then delete.'),
    );
  });
});
