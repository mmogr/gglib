/**
 * The chat page as a notebook: each turn a row of margin and body, the
 * margin saying only what the page has for that turn.
 *
 * The rule under test is "only true figures": a figure the page does not
 * have is left out, never drawn as zero or a dash. A saved reply has its
 * time, how long it thought and its tool calls; nothing here knows its model
 * or token counts. A reply arriving has how far its prompt was read.
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
    expect(within(row).getByText('thought 19s')).toBeInTheDocument();
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
    const made = within(row).getByText('thought 19s');

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
    expect(within(detail).getByText(/Thought for 19\.0s/)).toBeInTheDocument();
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
});
