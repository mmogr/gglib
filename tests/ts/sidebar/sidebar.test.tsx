/**
 * The sidebar on every screen: its rows go through `app/paths`, the open
 * row is read from the address, Library counts the library, Settings opens
 * the settings place, and Diagnostics opens the diagnostics place on Live. Under 720 px
 * the sidebar is a drawer. Its chats are `chatList.test.tsx`'s.
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
  revealSecret: vi.fn(async () => null),
  listConversations: vi.fn(async (): Promise<unknown[]> => []),
  listRuns: vi.fn(async () => []),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));
vi.mock('../../../src/places/diagnostics/DiagnosticsPlace', () => ({ default: () => <main aria-label="Diagnostics" /> }));

import { resetChats } from '../../../src/services/chatRegistry';
import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { renderShell } from '../places/harness';
import { placeOf } from '../../../src/sidebar/Sidebar';

const row = (name: string) => screen.getByRole('button', { name });

describe('the sidebar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetChats();
  });
  afterEach(() => {
    cleanupModelEvents();
  });

  it('is on every screen, and the open row is the address', async () => {
    const app = renderShell('/library');
    expect(screen.getByRole('navigation', { name: 'gglib' })).toBeInTheDocument();
    expect(row('Library')).toHaveAttribute('aria-current', 'page');

    await app.router.navigate('/diagnostics/benchmarks');
    await screen.findByRole('main', { name: 'Diagnostics' });
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

  it('Settings opens the settings place on General, marked current there; Diagnostics opens Live, marked current there', async () => {
    const app = renderShell('/library');
    const user = userEvent.setup();
    expect(row('Settings')).not.toHaveAttribute('aria-current');
    await user.click(row('Settings'));
    expect(app.at()).toBe('/settings/general');
    expect(app.router.state.historyAction).toBe('PUSH');
    expect(row('Settings')).toHaveAttribute('aria-current', 'page');
    expect(row('Library')).not.toHaveAttribute('aria-current');

    await user.click(row('Diagnostics'));
    await waitFor(() => expect(app.at()).toBe('/diagnostics/live'));
    expect(row('Diagnostics')).toHaveAttribute('aria-current', 'page');
    expect(row('Settings')).not.toHaveAttribute('aria-current');
  });

  describe('under 720 px, the sidebar is a drawer', () => {
    async function opened(path = '/library') {
      const app = renderShell(path);
      const user = userEvent.setup();
      expect(screen.queryByTestId('sidebar-drawer')).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Open the sidebar' }));
      expect(screen.getByTestId('sidebar-drawer')).toBeInTheDocument();
      // The sidebar is drawn twice: the one the wide window shows (hidden by
      // the stylesheet under 720 px) and the drawer's.
      expect(screen.getAllByRole('navigation', { name: 'gglib' })).toHaveLength(2);
      return { app, user };
    }
    const gone = () => waitFor(() => expect(screen.queryByTestId('sidebar-drawer')).not.toBeInTheDocument());

    it('closes on a move to another place', async () => {
      const { app, user } = await opened();
      await user.click(screen.getAllByRole('button', { name: 'Diagnostics' })[1]);
      await waitFor(() => expect(app.at()).toBe('/diagnostics/live'));
      await gone();
    });

    it('closes on a move to the settings place from it', async () => {
      const { app, user } = await opened();
      await user.click(screen.getAllByRole('button', { name: 'Settings' })[1]);
      await waitFor(() => expect(app.at()).toBe('/settings/general'));
      await gone();
    });

    it('closes on the scrim, and on its own close button', async () => {
      const { user } = await opened();
      await user.click(screen.getByTestId('sidebar-scrim'));
      await gone();

      await user.click(screen.getByRole('button', { name: 'Open the sidebar' }));
      await user.click(screen.getByRole('button', { name: 'Close the sidebar' }));
      await gone();
    });

    it('closes on Escape, unless a dialog has already handled that keypress', async () => {
      const { user } = await opened();
      // A dialog consumes Escape on a capture listener and marks it handled.
      const consume = (event: KeyboardEvent) => {
        if (event.key === 'Escape') event.preventDefault();
      };
      document.addEventListener('keydown', consume, true);
      await user.keyboard('{Escape}');
      expect(screen.getByTestId('sidebar-drawer')).toBeInTheDocument();
      document.removeEventListener('keydown', consume, true);

      await user.keyboard('{Escape}');
      await gone();
    });
  });
});

describe('placeOf', () => {
  it('maps each address to its row', () => {
    expect(placeOf('/library')).toBe('library');
    expect(placeOf('/models/7')).toBe('library');
    expect(placeOf('/far/3')).toBe('library');
    expect(placeOf('/endpoint')).toBe('endpoint');
    expect(placeOf('/devices')).toBe('devices');
    expect(placeOf('/chat/7')).toBe('chats');
    expect(placeOf('/chat/far/3')).toBe('chats');
    expect(placeOf('/diagnostics/benchmarks')).toBe('diagnostics');
    expect(placeOf('/')).toBeNull();
  });
});
