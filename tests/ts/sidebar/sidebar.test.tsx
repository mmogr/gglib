/**
 * The sidebar on every screen: its rows go through `app/paths`, the open
 * row is read from the address, Library counts the library, Chats opens the
 * latest chat on its model, Settings opens the dialog over the place, and
 * Diagnostics opens the benchmarks. Under 720 px the sidebar is a drawer.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { guiModel } from '../fixtures/model';

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(() => () => {}),
  listModels: vi.fn(async () => [guiModel({ id: 7, name: 'qwen3-8b' }), guiModel({ id: 9, name: 'gemma-3-12b' })]),
  getSettings: vi.fn(async () => ({})),
  getProxyStatus: vi.fn(async () => ({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null })),
  getRemoteStatus: vi.fn(async () => null),
}));
const chats = vi.hoisted(() => ({
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/transport/api/chat', () => ({
  listConversations: () => chats.listConversations(),
}));
vi.mock('../../../src/pages/ModelControlCenterPage', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/app/ChatRoute', () => ({ ChatRoute: () => <main aria-label="Chat" />, CHAT_LOADING: null }));
vi.mock('../../../src/app/BenchmarkRoute', () => ({ BenchmarkRoute: () => <main aria-label="Benchmarks" /> }));

import { peekChatHandoff, clearChatHandoff } from '../../../src/app/chatHandoff';
import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { renderShell } from '../places/harness';
import { placeOf } from '../../../src/sidebar/Sidebar';

/** A conversation as the daemon lists one; only these fields are read. */
function chat(id: number, model_id: number | null, updated_at: string) {
  return { id, title: `chat ${id}`, model_id, system_prompt: null, created_at: updated_at, updated_at };
}

const row = (name: string) => screen.getByRole('button', { name });

describe('the sidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    chats.listConversations.mockResolvedValue([]);
  });
  afterEach(() => {
    cleanupModelEvents();
    clearChatHandoff(7);
  });

  it('is on every screen, and the open row is the address', async () => {
    const app = renderShell('/library');
    expect(screen.getByRole('navigation', { name: 'gglib' })).toBeInTheDocument();
    expect(row('Library')).toHaveAttribute('aria-current', 'page');
    expect(row('Chats')).not.toHaveAttribute('aria-current');

    await app.router.navigate('/diagnostics/benchmarks');
    await screen.findByRole('main', { name: 'Benchmarks' });
    expect(row('Diagnostics')).toHaveAttribute('aria-current', 'page');
    expect(row('Library')).not.toHaveAttribute('aria-current');
  });

  it('Library counts the library once it is read, and goes home', async () => {
    const app = renderShell('/diagnostics/benchmarks');
    initModelEvents();
    await waitFor(() => expect(screen.getByRole('button', { name: /^Library/ })).toHaveTextContent('2'));

    await userEvent.setup().click(screen.getByRole('button', { name: /^Library/ }));
    expect(app.at()).toBe('/library');
  });

  it('Chats opens the newest chat that has a model, handed to the chat route', async () => {
    chats.listConversations.mockResolvedValue([
      chat(3, 9, '2026-11-05T10:00:00Z'),
      chat(5, null, '2026-11-06T12:00:00Z'),
      chat(4, 7, '2026-11-06T09:00:00Z'),
    ]);
    const app = renderShell('/library');
    await userEvent.setup().click(row('Chats'));

    await waitFor(() => expect(app.at()).toBe('/chat/7'));
    expect(app.router.state.location.state).toEqual({ handoff: true });
    expect(peekChatHandoff(7)).toEqual({ modelId: 7, conversationId: 4 });
  });

  it('Chats with no chats yet, or a list that cannot be read, stays in the library', async () => {
    const app = renderShell('/diagnostics/benchmarks');
    await userEvent.setup().click(row('Chats'));
    await waitFor(() => expect(app.at()).toBe('/library'));

    chats.listConversations.mockRejectedValue(new Error('daemon gone'));
    await app.router.navigate('/diagnostics/benchmarks');
    await userEvent.setup().click(row('Chats'));
    await waitFor(() => expect(app.at()).toBe('/library'));
  });

  it('Settings opens the dialog over the place, replacing the entry; Diagnostics opens the benchmarks', async () => {
    const app = renderShell('/library');
    const user = userEvent.setup();
    await user.click(row('Settings'));
    expect(app.at()).toBe('/library?settings=general');
    expect(app.router.state.historyAction).toBe('REPLACE');

    await user.click(row('Diagnostics'));
    await waitFor(() => expect(app.at()).toBe('/diagnostics/benchmarks'));
  });

  it('under 720 px, the sidebar is a drawer behind a menu button, closed by a move', async () => {
    const app = renderShell('/library');
    const user = userEvent.setup();
    expect(screen.queryByTestId('sidebar-drawer')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Open the sidebar' }));
    const drawer = screen.getByTestId('sidebar-drawer');
    expect(drawer).toBeInTheDocument();
    expect(screen.getAllByRole('navigation', { name: 'gglib' })).toHaveLength(2);

    // A move to another place closes it.
    await user.click(screen.getAllByRole('button', { name: 'Diagnostics' })[1]);
    await waitFor(() => expect(app.at()).toBe('/diagnostics/benchmarks'));
    await waitFor(() => expect(screen.queryByTestId('sidebar-drawer')).not.toBeInTheDocument());
  });
});

describe('placeOf', () => {
  it('maps each address to its row', () => {
    expect(placeOf('/library')).toBe('library');
    expect(placeOf('/models/7')).toBe('library');
    expect(placeOf('/far/3')).toBe('library');
    expect(placeOf('/chat/7')).toBe('chats');
    expect(placeOf('/chat/far/3')).toBe('chats');
    expect(placeOf('/diagnostics/benchmarks')).toBe('diagnostics');
    expect(placeOf('/')).toBeNull();
  });
});
