/**
 * The conversation list beside the notebook, and the rail that folds it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { agentRun, chatTransport, conversation, wrapper, type ChatFixture } from './chatPageHarness';
import { UNREAD_STORAGE_KEY } from '../../../src/components/ConversationListPanel/useConversationActivity';

const transport = vi.hoisted(() => ({ current: {} as unknown }));
vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return { ...actual, getTransport: () => transport.current };
});

import ChatPage from '../../../src/pages/ChatPage';

let fixture: ChatFixture;

function renderPage() {
  return render(<ChatPage modelName="qwen3" modelId={7} serverPort={4321} onClose={async () => {}} />, { wrapper });
}

beforeEach(() => {
  window.localStorage.clear();
  fixture = {
    conversations: [conversation(1, 'launchd KeepAlive'), conversation(2, 'Parsing GGUF')],
    rows: {},
    runs: [],
    frames: {},
  };
  transport.current = chatTransport(fixture);
});

describe('ChatPage, conversation list', () => {
  it('shows the list beside the notebook, folds it from the rail, and remembers', async () => {
    const user = userEvent.setup();
    const page = renderPage();
    const list = await screen.findByRole('listbox', { name: 'Conversations' });
    const button = screen.getByRole('button', { name: /^Conversations/ });
    expect(list).toBeVisible();
    expect(button).toHaveAttribute('aria-expanded', 'true');

    await user.click(button);
    expect(list).not.toBeVisible();
    expect(button).toHaveAttribute('aria-expanded', 'false');

    page.unmount();
    renderPage();
    const again = await screen.findByRole('listbox', { name: 'Conversations', hidden: true });
    expect(again).not.toBeVisible();
    expect(screen.getByRole('button', { name: /^Conversations/ })).toHaveAttribute('aria-expanded', 'false');
  });

  it('unfolds the list to search it', async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /^Conversations/ }));
    await user.click(screen.getByRole('button', { name: 'Search conversations' }));

    const search = screen.getByRole('searchbox', { name: 'Search conversations' });
    expect(search).toBeVisible();
    await waitFor(() => expect(search).toHaveFocus());
  });

  it('keeps the view switcher and Close with the notebook, not the list', async () => {
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByRole('button', { name: /^Conversations/ }));
    const title = await screen.findByRole('heading', { name: 'launchd KeepAlive' });
    const head = title.closest('.grid') as HTMLElement;
    expect(within(head).getByRole('tab', { name: /chat/i })).toBeVisible();
    expect(within(head).getByRole('button', { name: 'Close' })).toBeVisible();
  });

  it('marks a conversation Running and one New, in words, on its row and the rail', async () => {
    const user = userEvent.setup();
    fixture.conversations.push(conversation(3, 'Quantisation notes'));
    fixture.runs = [agentRun('r2', 2, 'in_progress')];
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 3: 1_000 }));
    renderPage();

    const running = await screen.findByRole('option', { name: /Parsing GGUF/ });
    await waitFor(() => expect(within(running).getByText('Running')).toBeInTheDocument());
    const unseen = screen.getByRole('option', { name: /Quantisation notes/ });
    expect(within(unseen).getByText('New')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Conversations, 1 running, 1 new' })).toBeInTheDocument();

    // Showing it clears the mark.
    await user.click(unseen);
    await waitFor(() => expect(within(unseen).queryByText('New')).not.toBeInTheDocument());
    expect(screen.getByRole('button', { name: 'Conversations, 1 running' })).toBeInTheDocument();
    expect(JSON.parse(window.localStorage.getItem(UNREAD_STORAGE_KEY)!)).toEqual({});
  });

  it('counts only listed conversations: a mark kept for a deleted one is not counted', async () => {
    fixture.runs = [agentRun('r2', 2, 'in_progress')];
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 99: 1_000 }));
    renderPage();

    await screen.findByRole('button', { name: 'Conversations, 1 running' });
    expect(screen.queryByText('New')).not.toBeInTheDocument();
  });
});
