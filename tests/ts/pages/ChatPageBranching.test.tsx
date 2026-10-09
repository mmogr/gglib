/**
 * Edits, branches and Retry on the chat page (ADR 0017). A reply is edited
 * in place on screen, by Save; a question by Send. An edit that would
 * rewrite a saved reply is saved on a new branch of the chat, which the page
 * lists and opens, saying the original is kept; so is Branch from here. A
 * turn where the chat's family parts shows its options in the margin, and
 * one chosen opens its chat; the list marks a branch. A chat that ends in a
 * question nothing answers offers Retry beneath it, which answers it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { ReactNode } from 'react';
import { useToastContext } from '../../../src/contexts/ToastContext';
import type { ChatMessage } from '../../../src/services/transport';
import type { AgentRunRequest } from '../../../src/types/generated/AgentRunRequest';
import type { BranchPoint } from '../../../src/types/generated/BranchPoint';
import type { ChatChange } from '../../../src/types/generated/ChatChange';
import { agentRun, chatTransport, conversation, wrapper as pageWrapper, type ChatFixture } from './chatPageHarness';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

/** The toasts, which `ToastProvider` holds but does not draw. */
const ToastProbe = () => {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
};
const wrapper = ({ children }: { children: ReactNode }) =>
  pageWrapper({ children: <><ToastProbe />{children}</> });

let fixture: ChatFixture;
let nextRow: number;
/** The branch points each chat's thread says, by chat. */
let points: Record<number, BranchPoint[]>;

type Stub = ReturnType<typeof pageTransport>;
const stub = () => transport.current as Stub;

function save(conversationId: number, role: 'user' | 'assistant', content: string) {
  const row: ChatMessage = { id: nextRow++, conversation_id: conversationId, role, content, created_at: '2026-10-08T09:00:00Z' };
  fixture.rows[conversationId] = [...(fixture.rows[conversationId] ?? []), row];
}

function pageTransport() {
  const base = chatTransport(fixture);
  return {
    ...base,
    getThread: vi.fn(async (id: number) => {
      const messages = fixture.rows[id] ?? [];
      const along = points[id] ?? [];
      return {
        messages,
        ...(along.length > 0 && { points: along }),
        ...(messages.at(-1)?.role === 'user' && { answerable: true }),
      };
    }),
    // An edit of a reply, or Branch from here, as the daemon makes either:
    // a new chat holding the chat as far as the change, then the reply as
    // written.
    changeConversation: vi.fn(async (id: number, change: ChatChange) => {
      const branch = Math.max(...fixture.conversations.map((c) => c.id)) + 1;
      fixture.conversations.unshift({ ...conversation(branch, 'Kyoto'), branch_of: id });
      const at = fixture.rows[id].findIndex((r) => r.id === change.message_id);
      const kept = change.kind === 'edit' ? at : at + 1;
      fixture.rows[id].slice(0, kept).forEach((r) => save(branch, r.role as 'user', r.content));
      if (change.kind === 'edit') save(branch, 'assistant', change.content);
      return { conversation_id: branch, forked: true, answer: false };
    }),
    startAgentRun: vi.fn(async (id: string, request: AgentRunRequest) => {
      const conversationId = request.conversation_id as number;
      fixture.runs = [agentRun(id, conversationId, 'in_progress'), ...fixture.runs];
      return agentRun(id, conversationId, 'queued');
    }),
  };
}

beforeEach(() => {
  nextRow = 1;
  points = {};
  fixture = { conversations: [conversation(1, 'Kyoto')], rows: {}, runs: [], frames: {} };
  save(1, 'user', 'Plan a trip to Kyoto');
  save(1, 'assistant', 'Day 1: temples');
  transport.current = pageTransport();
});

async function renderPage() {
  render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
  await screen.findByText('Day 1: temples');
}

describe('ChatPage edits and Retry', () => {
  it('an edited reply is saved on a new branch, which opens, and the page says the original is kept', async () => {
    const user = userEvent.setup();
    await renderPage();

    await user.click(screen.getByRole('button', { name: 'Edit reply' }));
    const box = await screen.findByRole('textbox', { name: 'Edit reply' });
    await user.clear(box);
    await user.type(box, 'Day 1: gardens');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(screen.getByTestId('toasts')).toHaveTextContent('Saved as a new branch. The original is kept.'));
    expect(stub().changeConversation).toHaveBeenCalledWith(1, { kind: 'edit', message_id: 2, content: 'Day 1: gardens' });
    expect(await screen.findByText('Day 1: gardens')).toBeInTheDocument();
    await waitFor(() => expect(stub().getThread).toHaveBeenLastCalledWith(2));
    expect(fixture.rows[1].map((r) => r.content)).toEqual(['Plan a trip to Kyoto', 'Day 1: temples']);
    expect(stub().startAgentRun).not.toHaveBeenCalled();
  });

  it('an edit of a question is sent, by Send', async () => {
    const user = userEvent.setup();
    await renderPage();

    await user.click(screen.getByRole('button', { name: 'Edit message' }));
    expect(await screen.findByRole('textbox', { name: 'Edit message' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
  });

  it('a question nothing answers offers Retry, which answers it', async () => {
    save(1, 'user', 'Make it cheaper');
    const user = userEvent.setup();
    await renderPage();

    expect(await screen.findByText('Nothing answers this question yet.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => expect(stub().startAgentRun).toHaveBeenCalledTimes(1));
    expect(stub().startAgentRun.mock.calls[0][1]).toMatchObject({ conversation_id: 1, answer_saved: true, messages: [] });
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Retry' })).not.toBeInTheDocument());
  });

  it('a turn where the branches part shows them in its margin, and the next one opens its chat', async () => {
    fixture.conversations.unshift({ ...conversation(2, 'Kyoto'), branch_of: 1 });
    save(2, 'user', 'Plan a trip to Kyoto');
    save(2, 'assistant', 'Day 1: gardens');
    const options = [
      { conversation_id: 1, message_id: 2, role: 'assistant' as const, preview: 'Day 1: temples' },
      { conversation_id: 2, message_id: 4, role: 'assistant' as const, preview: 'Day 1: gardens' },
    ];
    points = { 1: [{ message_id: 2, index: 0, options }], 2: [{ message_id: 4, index: 1, options }] };
    const user = userEvent.setup();
    render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} conversationId={1} />, { wrapper });
    await screen.findByText('Day 1: temples');

    expect(screen.getByRole('button', { name: 'Branch 1 of 2' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Next branch' }));

    expect(await screen.findByText('Day 1: gardens')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Branch 2 of 2' })).toBeInTheDocument();
    expect(fixture.rows[1].map((r) => r.content)).toEqual(['Plan a trip to Kyoto', 'Day 1: temples']);
    // The chat opened is the one the list marks a branch.
    await waitFor(() => expect(within(screen.getByRole('option', { selected: true })).getByText('Branch')).toBeInTheDocument());
    expect(screen.getAllByRole('option').filter((row) => within(row).queryByText('Branch'))).toHaveLength(1);
  });

  it('Branch from here copies the chat as far as that turn into a new branch, which opens', async () => {
    const user = userEvent.setup();
    await renderPage();

    await user.click(screen.getAllByRole('button', { name: 'Branch from here' })[1]);

    await waitFor(() => expect(stub().changeConversation).toHaveBeenCalledWith(1, { kind: 'branch', message_id: 2 }));
    await waitFor(() => expect(stub().getThread).toHaveBeenLastCalledWith(2));
    expect(stub().startAgentRun).not.toHaveBeenCalled();
  });

  it('a chat that ends in a reply offers no Retry', async () => {
    await renderPage();
    expect(screen.queryByText('Nothing answers this question yet.')).not.toBeInTheDocument();
  });
});
