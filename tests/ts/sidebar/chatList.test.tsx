/**
 * The sidebar's chats: newest first, title only, the open one marked; a
 * running reply's arc and an unseen reply's bold; ⌕ filters; + starts a chat
 * at once; ⋯ renames and deletes (asked first); a joined machine's chats as
 * an "On desk" group at the foot. The list scrolls inside the sidebar and
 * its foot stays put.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

interface Chat {
  id: number;
  title: string;
  model_id: number | null;
  system_prompt: null;
  created_at: string;
  updated_at: string;
}
const daemon = vi.hoisted(() => ({ chats: [] as Chat[], runs: [] as unknown[] }));

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => []),
  getSettings: vi.fn(async () => ({})),
  getProxyStatus: vi.fn(async () => ({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null })),
  getRemoteStatus: vi.fn(async () => null),
  revealSecret: vi.fn(async () => null),
  listPairedModels: vi.fn(async () => ({ machine: { kind: 'paired', fingerprint: '3ca82708b995' }, actions: [], models: [] })),
  listConversations: vi.fn(async (): Promise<unknown[]> => daemon.chats.map((c) => ({ ...c }))),
  listRuns: vi.fn(async () => daemon.runs),
  listFarChats: vi.fn(async () => [
    { id: 4, title: 'Why the build broke', updated_at: '2026-11-06 10:00:00' },
    { id: 5, title: 'Morning summary', updated_at: '2026-11-05 10:00:00' },
  ]),
  listFarRuns: vi.fn(async () => []),
  deleteConversation: vi.fn(async (id: number) => {
    daemon.chats = daemon.chats.filter((c) => c.id !== id);
  }),
  updateConversationTitle: vi.fn(async (id: number, title: string) => {
    daemon.chats = daemon.chats.map((c) => (c.id === id ? { ...c, title } : c));
  }),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/components/ProxyDashboardModal', () => ({ ProxyDashboardModal: () => null }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/app/ChatRoute', () => ({ ChatRoute: () => <main aria-label="Chat" /> }));
vi.mock('../../../src/app/FarChatRoute', () => ({ FarChatRoute: () => <main aria-label="Far chat" /> }));

import { resetChats } from '../../../src/services/chatRegistry';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import { UNREAD_STORAGE_KEY } from '../../../src/sidebar/useConversationActivity';
import { renderShell } from '../places/harness';

function chat(id: number, title: string, updated_at: string, model_id: number | null = 7): Chat {
  return { id, title, model_id, system_prompt: null, created_at: updated_at, updated_at };
}

const chats = () => screen.getByRole('region', { name: 'Chats' });
/** The rows of this machine's chats, in order, by title. */
const titles = () =>
  within(chats())
    .getAllByRole('listitem')
    .filter((li) => !li.closest('[aria-label^="On "]'))
    .map((li) => li.textContent);
const rowButton = (title: string) => within(chats()).getByRole('button', { name: title });
const rowOf = (title: string) => rowButton(title).closest('li') as HTMLElement;
const marks = () => JSON.parse(window.localStorage.getItem(UNREAD_STORAGE_KEY) ?? '{}');

beforeEach(() => {
  vi.clearAllMocks();
  window.localStorage.clear();
  resetChats();
  resetRemoteState();
  daemon.runs = [];
  daemon.chats = [
    chat(1, 'Five largest files, in bash', '2026-11-05 09:00:00'),
    chat(2, 'Rust lifetimes', '2026-11-06 12:00:00'),
    chat(3, 'Quantisation notes', '2026-11-04 08:00:00', 9),
  ];
});
afterEach(() => resetRemoteState());

describe('the sidebar chats', () => {
  it('lists the chats newest first, title only, and marks the open one', async () => {
    renderShell('/chat/1');
    await waitFor(() => expect(titles()).toEqual(['Rust lifetimes', 'Five largest files, in bash', 'Quantisation notes']));
    expect(rowButton('Five largest files, in bash')).toHaveAttribute('aria-current', 'page');
    expect(rowButton('Rust lifetimes')).not.toHaveAttribute('aria-current');
  });

  it('shows a skeleton while the list is read, said as loading', async () => {
    let answer: () => void = () => {};
    transport.listConversations.mockImplementationOnce(() => new Promise((resolve) => { answer = () => resolve([]); }));
    renderShell('/library');
    expect(within(chats()).getByRole('status', { name: 'Loading chats' })).toBeInTheDocument();
    await act(async () => answer());
    expect(await within(chats()).findByText('No chats yet.')).toBeInTheDocument();
  });

  it('opens a chat by its address, and the row it opened is the one marked', async () => {
    const user = userEvent.setup();
    const app = renderShell('/library');
    await user.click(await within(chats()).findByRole('button', { name: 'Rust lifetimes' }));
    expect(app.at()).toBe('/chat/2');
    expect(rowButton('Rust lifetimes')).toHaveAttribute('aria-current', 'page');
  });

  it('a running reply adds the arc; an unseen reply is bold until its chat is opened', async () => {
    const user = userEvent.setup();
    daemon.runs = [{ id: 'r1', kind: 'agent', status: 'in_progress', created_at_ms: 1, conversation_id: 2, last_seq: 0 }];
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 3: 1_000 }));
    renderShell('/library');

    await waitFor(() => expect(within(rowOf('Rust lifetimes')).getByRole('img', { name: 'Reply running' })).toBeInTheDocument());
    expect(within(rowOf('Quantisation notes')).queryByRole('img', { name: 'Reply running' })).not.toBeInTheDocument();
    expect(rowButton('Quantisation notes')).toHaveClass('font-semibold');
    expect(rowButton('Five largest files, in bash')).not.toHaveClass('font-semibold');

    await user.click(rowButton('Quantisation notes'));
    await waitFor(() => expect(rowButton('Quantisation notes')).not.toHaveClass('font-semibold'));
    expect(marks()).toEqual({});
  });

  it("drops the mark of a chat the daemon no longer lists, once the list is read", async () => {
    window.localStorage.setItem(UNREAD_STORAGE_KEY, JSON.stringify({ 99: 1_000 }));
    renderShell('/library');
    await waitFor(() => expect(titles()).toHaveLength(3));
    await waitFor(() => expect(marks()).toEqual({}));
  });

  it('⌕ shows a filter field that narrows the list by title, and says when nothing matches', async () => {
    const user = userEvent.setup();
    renderShell('/library');
    await waitFor(() => expect(titles()).toHaveLength(3));
    expect(within(chats()).queryByRole('searchbox')).not.toBeInTheDocument();

    await user.click(within(chats()).getByRole('button', { name: 'Filter chats' }));
    const field = within(chats()).getByRole('searchbox', { name: 'Filter chats' });
    await waitFor(() => expect(field).toHaveFocus());
    await user.type(field, 'RUST');
    expect(titles()).toEqual(['Rust lifetimes']);

    await user.clear(field);
    await user.type(field, 'kyoto');
    expect(within(chats()).getByText('No matching chats')).toBeInTheDocument();
    expect(within(chats()).queryByText('No chats yet.')).not.toBeInTheDocument();
  });

  it('says "No chats yet." with none', async () => {
    daemon.chats = [];
    renderShell('/library');
    expect(await within(chats()).findByText('No chats yet.')).toBeInTheDocument();
  });

  it('+ starts a chat at once: an empty one, no dialog', async () => {
    const user = userEvent.setup();
    const app = renderShell('/library');
    await user.click(within(chats()).getByRole('button', { name: 'New chat' }));
    expect(app.at()).toBe('/chat/new');
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('⋯ Rename renames the chat in place', async () => {
    const user = userEvent.setup();
    renderShell('/library');
    await waitFor(() => expect(titles()).toHaveLength(3));

    await user.click(within(chats()).getByRole('button', { name: 'More for Rust lifetimes' }));
    await user.click(screen.getByRole('menuitem', { name: 'Rename' }));
    const field = within(chats()).getByRole('textbox', { name: 'Chat title' });
    await user.clear(field);
    await user.type(field, 'Borrow checker{Enter}');

    await waitFor(() => expect(transport.updateConversationTitle).toHaveBeenCalledWith(2, 'Borrow checker'));
    await waitFor(() => expect(titles()[0]).toBe('Borrow checker'));
  });

  it('⋯ Delete… asks first, and deletes only on a yes', async () => {
    const user = userEvent.setup();
    const app = renderShell('/chat/1');
    await waitFor(() => expect(titles()).toHaveLength(3));

    await user.click(within(chats()).getByRole('button', { name: 'More for Rust lifetimes' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(transport.deleteConversation).not.toHaveBeenCalled();

    await user.click(within(chats()).getByRole('button', { name: 'More for Rust lifetimes' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    await user.click(await screen.findByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(titles()).toEqual(['Five largest files, in bash', 'Quantisation notes']));
    expect(transport.deleteConversation).toHaveBeenCalledWith(2);
    // Another chat was deleted: the open one stays open.
    expect(app.at()).toBe('/chat/1');
  });

  it('deleting the open chat opens an empty one on its model', async () => {
    const user = userEvent.setup();
    const app = renderShell('/chat/3');
    await waitFor(() => expect(titles()).toHaveLength(3));

    await user.click(within(chats()).getByRole('button', { name: 'More for Quantisation notes' }));
    await user.click(screen.getByRole('menuitem', { name: 'Delete…' }));
    await user.click(await screen.findByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(app.at()).toBe('/chat/new?model=9'));
  });

  it("a joined machine's chats are an \"On desk\" group at the foot, opened by their own address, not changed here", async () => {
    const user = userEvent.setup();
    act(() =>
      applyRemoteStatus({
        ...IDLE_STATUS,
        paired_name: 'desk',
        connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: '3ca82708b995', path: 'direct', away_for_s: null },
      }),
    );
    const app = renderShell('/library');
    const desk = await within(chats()).findByRole('region', { name: 'On desk' });
    expect(within(desk).getAllByRole('listitem').map((li) => li.textContent)).toEqual(['Why the build broke', 'Morning summary']);
    expect(within(desk).queryByRole('button', { name: /^More for/ })).not.toBeInTheDocument();
    // At the foot: after this machine's chats.
    const ownList = rowOf('Rust lifetimes').closest('ul') as HTMLElement;
    expect(ownList.compareDocumentPosition(desk) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    await user.click(within(desk).getByRole('button', { name: 'Why the build broke' }));
    expect(app.at()).toBe('/chat/far/4');
    expect(within(desk).getByRole('button', { name: 'Why the build broke' })).toHaveAttribute('aria-current', 'page');
  });

  it('with no machine joined there is no "On" group, and the far machine is not asked', async () => {
    renderShell('/library');
    await waitFor(() => expect(titles()).toHaveLength(3));
    expect(within(chats()).queryByRole('region', { name: /^On / })).not.toBeInTheDocument();
    expect(transport.listFarChats).not.toHaveBeenCalled();
  });

  it('the list scrolls inside the sidebar; Settings and Diagnostics stay at the foot', async () => {
    renderShell('/library');
    await waitFor(() => expect(titles()).toHaveLength(3));
    const scroller = rowOf('Rust lifetimes').closest('.overflow-y-auto') as HTMLElement;
    expect(scroller).toHaveClass('min-h-0');
    expect(chats()).toHaveClass('min-h-0', 'flex-1');
    const settings = screen.getByRole('button', { name: 'Settings' });
    expect(scroller).not.toContainElement(settings);
    expect(settings.parentElement).toHaveClass('shrink-0');
  });

  it('reads the list again when a reply ends, so a new title and order show', async () => {
    daemon.runs = [{ id: 'r1', kind: 'agent', status: 'in_progress', created_at_ms: 1, conversation_id: 3, last_seq: 0 }];
    renderShell('/library');
    await waitFor(() => expect(within(rowOf('Quantisation notes')).getByRole('img', { name: 'Reply running' })).toBeInTheDocument());

    daemon.runs = [{ id: 'r1', kind: 'agent', status: 'completed', created_at_ms: 1, finished_at_ms: 2, conversation_id: 3, last_seq: 0 }];
    daemon.chats = daemon.chats.map((c) => (c.id === 3 ? { ...c, title: 'Q4_K_M or Q5_K_M', updated_at: '2026-11-07 09:00:00' } : c));
    // The next poll sees the run ended.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 3100));
    });
    await waitFor(() => expect(titles()[0]).toBe('Q4_K_M or Q5_K_M'));
  }, 10_000);
});
