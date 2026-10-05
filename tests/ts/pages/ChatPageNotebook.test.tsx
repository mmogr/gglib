/**
 * The chat page as a notebook: each turn a row of margin and body, the
 * margin saying only what the page has for that turn.
 *
 * The rule under test is "only true figures": a figure the page does not
 * have is left out, never drawn as zero or a dash. A saved reply has its
 * time, how long it thought and its tool calls; nothing here knows its model
 * or token counts. A reply arriving has how far its prompt was read.
 *
 * The composer's context ring keeps the same rule: drawn from the figures
 * the last reply carried, and not at all where its context size is missing.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ChatMessage } from '../../../src/services/transport';
import { agentRun, chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

function savedExchange(): ChatMessage[] {
  return [
    { id: 11, conversation_id: 1, role: 'user', content: 'What does KeepAlive do?', created_at: '2026-09-01T09:12:00Z' },
    {
      id: 12,
      conversation_id: 1,
      role: 'assistant',
      content: 'It restarts the job whenever it exits.',
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

let fixture: ChatFixture;

function renderLocal() {
  return render(
    <ChatPage modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onClose={async () => {}} />,
    { wrapper },
  );
}

/** The row an element is in: the turn's grid. */
function rowOf(element: HTMLElement): HTMLElement {
  return element.closest('.grid') as HTMLElement;
}

beforeEach(() => {
  window.localStorage.clear();
  fixture = { conversations: [conversation(1, 'launchd KeepAlive')], rows: { 1: savedExchange() }, runs: [], frames: {} };
  transport.current = chatTransport(fixture);
});

describe('ChatPage, notebook', () => {
  it('draws a saved reply with the figures it has, and none it does not', async () => {
    renderLocal();
    const body = await screen.findByText('It restarts the job whenever it exits.');
    const row = rowOf(body);

    expect(within(row).getByText('Assistant')).toBeInTheDocument();
    expect(within(row).getByText('thought 19.0s')).toBeInTheDocument();
    expect(within(row).getByText('1 tool call')).toBeInTheDocument();
    expect(within(row).getByRole('button', { name: 'How this was made' })).toBeInTheDocument();

    // Saved rows keep no token counts, speed or model: nothing stands in
    // for them, not a zero and not a dash.
    expect(row).not.toHaveTextContent(/tok read|from cache|tok\/s|Qwen3\.8-27B|—/);
    expect(row).not.toHaveTextContent(/\b0\b/);
  });

  it('reads who, then the body, then how it was made', async () => {
    renderLocal();
    const body = await screen.findByText('It restarts the job whenever it exits.');
    const row = rowOf(body);
    const who = within(row).getByText('Assistant');
    const made = within(row).getByText('thought 19.0s');

    const follows = (a: Node, b: Node) => Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
    expect(follows(who, body)).toBe(true);
    expect(follows(body, made)).toBe(true);
  });

  it('opens the reasoning and the tool calls from the margin', async () => {
    const user = userEvent.setup();
    renderLocal();
    const row = rowOf(await screen.findByText('It restarts the job whenever it exits.'));
    const toggle = within(row).getByRole('button', { name: 'How this was made' });

    expect(toggle).toHaveAttribute('aria-expanded', 'false');
    const detail = document.getElementById(toggle.getAttribute('aria-controls')!)!;
    expect(detail).not.toBeVisible();

    await user.click(toggle);
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    expect(detail).toBeVisible();
    // The margin says how long it thought; the block does not say it again.
    expect(within(detail).getByText('Reasoning')).toBeInTheDocument();
    expect(detail).not.toHaveTextContent(/Thought for/);
  });

  it('shows a reply arriving as how far its prompt has been read', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'prompt_progress', processed: 1240, total: 3420, cached: 2100, time_ms: 900 }];
    renderLocal();

    const status = (await screen.findByText('Reading the prompt')).closest('[role="status"]') as HTMLElement;
    expect(status).not.toBeNull();
    const row = rowOf(status);
    expect(within(row).getByText(/1,240 of 3,420/)).toBeInTheDocument();
    expect(within(row).getByText('2,100 from cache')).toBeInTheDocument();
    expect(within(row).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '36');
  });

  it('says nothing of the prompt before its first reading arrives', async () => {
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    renderLocal();

    const status = (await screen.findByText('Waiting for the model')).closest('[role="status"]') as HTMLElement;
    expect(status).not.toBeNull();
    expect(rowOf(status)).not.toHaveTextContent(/tok|from cache/);
  });

  it('puts the model and its quantisation in the composer margin', async () => {
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const row = rowOf(screen.getByRole('textbox', { name: 'Message' }));
    expect(within(row).getByText('Qwen3.8-27B')).toBeInTheDocument();
    expect(await within(row).findByText('Q8_0')).toBeInTheDocument();
  });

  it('keeps the model and its quantisation in the margin when the name is a picker', async () => {
    render(
      <ChatPage modelName="Qwen3.8-27B" modelId={7} serverPort={4321} onSwitchModel={async () => {}} onClose={async () => {}} />,
      { wrapper },
    );
    await screen.findByText('It restarts the job whenever it exits.');
    const row = rowOf(screen.getByRole('textbox', { name: 'Message' }));
    expect(within(row).getByRole('combobox', { name: 'Model' })).toHaveDisplayValue('Qwen3.8-27B');
    expect(await within(row).findByText('Q8_0')).toBeInTheDocument();
  });

  it("opens the composer's tools popout rightwards, over the page and not off it", async () => {
    const user = userEvent.setup();
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    const row = rowOf(screen.getByRole('textbox', { name: 'Message' }));

    await user.click(within(row).getByRole('button', { name: 'Tools' }));

    const popout = within(row).getByText(/active$/).closest('.z-popover');
    expect(popout).toHaveClass('left-0');
    expect(popout).not.toHaveClass('right-0');
  });

  it('names the model and says how the reply was made, in the mock-up order', async () => {
    fixture.rows[1][1].metadata = {
      ...fixture.rows[1][1].metadata,
      modelName: 'Qwen3.8-27B',
      modelQuantization: 'Q8_0',
      promptTokens: 3180,
      cachedTokens: 2100,
      completionTokens: 496,
      turnDurationMs: 41_000,
      writingDurationMs: 40_992,
    };
    renderLocal();
    const row = rowOf(await screen.findByText('It restarts the job whenever it exits.'));
    expect(within(row).getByText('Qwen3.8-27B')).toBeInTheDocument();
    expect(within(row).queryByText('Assistant')).not.toBeInTheDocument();
    expect(within(row).getByText(/· Q8_0$/)).toBeInTheDocument();
    const lines = within(row).getAllByRole('listitem').map((li) => li.textContent);
    expect(lines).toEqual(['thought 19.0s', '1 tool call', '3,180 tok read', '2,100 from cache', '41.0s · 12 tok/s']);
  });

  it('never draws a margin beside an empty body: a turn of only a tool call or only reasoning shows it', async () => {
    fixture.rows[1] = [
      savedExchange()[0],
      { ...savedExchange()[1], content: '', metadata: { tool_calls: [{ id: 'c1', name: 'read_file', arguments: {} }] } },
      savedExchange()[2],
      { id: 14, conversation_id: 1, role: 'assistant', content: '', created_at: '2026-09-01T09:14:00Z', metadata: { thinking: 'Only thought.', thinkingDurationSeconds: 2 } },
    ];
    renderLocal();
    const tool = rowOf(await screen.findByText('1 tool call'));
    expect(within(tool).getByText('read_file')).toBeVisible();
    expect(within(tool).queryByRole('button', { name: 'How this was made' })).not.toBeInTheDocument();
    const thought = rowOf(screen.getByText('thought 2.0s'));
    expect(within(thought).getByText('Reasoning')).toBeVisible();
    expect(within(thought).queryByRole('button', { name: 'How this was made' })).not.toBeInTheDocument();
  });

  it('never makes up a quantisation: a reply or a model without one shows none', async () => {
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, modelName: 'qwen3' };
    (transport.current as { getModel: () => Promise<unknown> }).getModel = async () => ({ quantization: null });
    renderLocal();
    const row = rowOf(await screen.findByText('It restarts the job whenever it exits.'));
    expect(within(row).getByText('qwen3')).toBeInTheDocument();
    const when = within(row).getByText((_, el) => el?.tagName === 'TIME');
    expect(when.parentElement?.textContent).toBe(when.textContent);
    const composer = rowOf(screen.getByRole('textbox', { name: 'Message' }));
    expect(composer).not.toHaveTextContent(/Q\d|·/);
  });
});

describe('ChatPage, the context ring', () => {
  /** The composer margin's ring, by its name; null when the page draws none. */
  const ring = () => screen.queryByRole('button', { name: /^Context: / });
  const detail = () => screen.queryByRole('group', { name: 'Context' });
  const composerRow = () => rowOf(screen.getByRole('textbox', { name: 'Message' }));
  /** The ring's track, the circle under its arc: there whenever a ring is drawn, named or not. */
  const track = () => composerRow().querySelector('circle.stroke-border');

  /**
   * What the ring draws: its arc, the second circle and the one over the
   * track, in this colour and as long as `used` of `size` round its radius.
   */
  function expectArc(trigger: HTMLElement, stroke: string, used: number, size: number) {
    const arc = trigger.querySelectorAll('circle')[1];
    expect(arc).toHaveClass(stroke);
    const radius = Number(arc.getAttribute('r'));
    expect(radius).toBeGreaterThan(0);
    expect(Number(arc.getAttribute('stroke-dashoffset'))).toBeCloseTo(2 * Math.PI * radius * (1 - used / size), 6);
  }

  /** Give the saved reply these figures, as its row keeps them. */
  function replyMade(figures: Record<string, unknown>) {
    fixture.rows[1][1].metadata = { ...fixture.rows[1][1].metadata, ...figures };
  }
  const QUARTER = { promptTokens: 8000, completionTokens: 200, contextSize: 32768 };

  it('draws no ring where the last reply has no figures', async () => {
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(ring()).not.toBeInTheDocument();
    // Nor a ring with no name: an empty one would still draw its track.
    expect(track()).toBeNull();
  });

  it('draws nothing where the last reply has counts and no context size: not a ring, a zero or a dash', async () => {
    replyMade({ promptTokens: 8000, completionTokens: 200 });
    renderLocal();
    await screen.findByText('8,000 tok read');
    expect(ring()).not.toBeInTheDocument();
    expect(track()).toBeNull();
    expect(composerRow()).not.toHaveTextContent(/%|—|\b0\b/);
  });

  it('draws the ring after the tools button, says the sentence on hover and opens the detail on a click', async () => {
    const user = userEvent.setup();
    replyMade(QUARTER);
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = within(composerRow()).getByRole('button', { name: 'Context: 25 percent of context used' });
    expect(trigger).toHaveAttribute('title', '8,200 of 32,768 tokens (25%) after the last finished reply.');
    // Under 70% the ring stands alone: no figure and no mark beside it.
    expect(trigger.textContent).toBe('');
    expect(trigger.querySelectorAll('svg')).toHaveLength(1);
    expect(track()).not.toBeNull();
    expectArc(trigger, 'stroke-primary', 8200, 32768);
    const tools = within(composerRow()).getByRole('button', { name: 'Tools' });
    expect(tools.compareDocumentPosition(trigger) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

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

  it('from 70% says the figure beside the ring with a warning mark, and the detail says why', async () => {
    const user = userEvent.setup();
    replyMade({ promptTokens: 24000, completionTokens: 400, contextSize: 32768, trimmedMessages: 3, finishReason: 'length' });
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = screen.getByRole('button', { name: 'Context: 74 percent of context used, filling up' });
    expect(trigger.textContent).toBe('74%');
    const figure = within(trigger).getByText('74%');
    expect(figure).toHaveClass('text-warning', 'font-mono', 'tabular-nums');
    expect(figure.querySelector('svg')).not.toBeNull();
    expectArc(trigger, 'stroke-warning', 24400, 32768);

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
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');

    const trigger = screen.getByRole('button', { name: 'Context: 90 percent of context used, almost full' });
    const figure = within(trigger).getByText('90%');
    expect(figure).toHaveClass('text-danger');
    expect(figure).not.toHaveClass('text-warning');
    expect(figure.querySelector('svg')).not.toBeNull();
    expectArc(trigger, 'stroke-danger', 29492, 32768);
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
    renderLocal();
    await screen.findByText('It also');
    expect(screen.getByRole('button', { name: 'Context: 25 percent of context used' })).toBeInTheDocument();
  });

  it('borrows no size for a newest reply that has its counts and none: no ring, though the reply before has one', async () => {
    replyMade(QUARTER);
    secondReply({ promptTokens: 9000, completionTokens: 300 });
    renderLocal();
    await screen.findByText('It also');
    expect(ring()).not.toBeInTheDocument();
  });

  it('holds the last reading while a reply arrives without its figures', async () => {
    replyMade(QUARTER);
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [{ type: 'prompt_progress', processed: 1240, total: 3420, cached: 2100, time_ms: 900 }];
    renderLocal();
    await screen.findByText('Reading the prompt');
    expect(screen.getByRole('button', { name: 'Context: 25 percent of context used' })).toBeInTheDocument();
  });

  it('takes a reply\'s figures when they come, while its run is still live', async () => {
    replyMade(QUARTER);
    fixture.runs = [agentRun('r1', 1, 'in_progress')];
    fixture.frames.r1 = [
      { type: 'text_delta', content: 'Still going.' },
      { type: 'turn_usage', prompt_tokens: 29000, completion_tokens: 492, context_size: 32768 },
    ];
    renderLocal();
    await screen.findByText('Still going.');
    // Waited for by its title, which is cheap to look for; the name is read once it is there.
    const trigger = await screen.findByTitle('29,492 of 32,768 tokens (90%) after the last finished reply.');
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
    renderLocal();
    await screen.findByText('It restarts the job whenever it exits.');
    expect(ring()).toBeInTheDocument();

    await user.click(screen.getByRole('option', { name: /Parsing GGUF/ }));
    await screen.findByText('A block of numbers.');
    expect(ring()).not.toBeInTheDocument();
  });
});
