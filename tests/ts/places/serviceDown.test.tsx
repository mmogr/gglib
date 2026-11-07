/**
 * Settings › About and the service-not-running screen, through the real
 * routes under the gate the window has (`ServiceGate`): the build and the
 * modified flag; Documentation opening the docs; Stop gglib… asked first,
 * nothing sent on Cancel, then the screen once the daemon stops answering,
 * and a refusal said in the daemon's words. The screen itself: on the
 * desktop the sentence and Start gglib, which calls `start_gglib` through
 * the platform boundary, a refusal in the app's words, and the place back
 * at its address once the daemon answers; in a browser "Lost gglib.
 * Reconnecting…" with nothing to press, gone when it answers; and the place,
 * not the screen, before the first answer. The poll is the real two-second
 * one, run on fake timers.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { Outlet, RouterProvider } from 'react-router';
import { appSettings } from '../fixtures/settings';

const fake = vi.hoisted(() => ({
  /** Whether the daemon answers: `null` holds every probe unanswered. */
  up: true as boolean | null,
  version: { semver: '0.31.0', display: '0.31.0 (abc1234)', sha: 'abc1234', fingerprint: 'abc1234' },
  shutdown: null as (() => Promise<void>) | null,
  desktop: false,
  starts: 0,
  startRefusal: null as string | null,
  opened: [] as string[],
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getTransport: () => ({
    getSettings: async () => appSettings(),
    getVersion: async () => fake.version,
    getProxyStatus: () => {
      if (fake.up === null) return new Promise(() => {});
      return fake.up ? Promise.resolve({ running: false }) : Promise.reject(new TypeError('Failed to fetch'));
    },
    shutdownDaemon: vi.fn(async () => {
      if (fake.shutdown) return fake.shutdown();
      fake.up = false;
      throw new TypeError('Failed to fetch');
    }),
    subscribe: () => () => {},
    onEventStreamOpen: () => () => {},
  }),
}));
vi.mock('../../../src/services/transport/api/settings', () => ({
  getSettings: async () => appSettings(),
  updateSettings: vi.fn(),
  installProfileTemplates: vi.fn(),
}));
vi.mock('../../../src/services/platform', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isDesktop: () => fake.desktop,
  startGglib: async () => {
    fake.starts += 1;
    if (fake.startRefusal) throw fake.startRefusal;
    fake.up = true;
  },
  openUrl: async (url: string) => void fake.opened.push(url),
}));
vi.mock('../../../src/sidebar/Sidebar', () => ({ Sidebar: () => <nav aria-label="gglib">sidebar</nav> }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { createAppRouter } from '../../../src/app/router';
import { ServiceGate } from '../../../src/app/ServiceDown';
import { TransportError } from '../../../src/services/transport/errors';
import { PlaceProviders } from './harness';

/** The place at `path` behind the window's gate. */
function renderGated(path: string) {
  const router = createAppRouter(
    <PlaceProviders>
      <ServiceGate>
        <Outlet />
      </ServiceGate>
    </PlaceProviders>,
    [path],
  );
  render(<RouterProvider router={router} />);
  return router;
}

/** Let the poll ask again. */
const nextProbe = () => act(() => vi.advanceTimersByTimeAsync(2000));

const DOWN = "gglib isn't running, so nothing can be served.";
const LOST = 'Lost gglib. Reconnecting…';

let user: ReturnType<typeof userEvent.setup>;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  Object.assign(fake, {
    up: true,
    version: { semver: '0.31.0', display: '0.31.0 (abc1234)', sha: 'abc1234', fingerprint: 'abc1234' },
    shutdown: null,
    desktop: false,
    starts: 0,
    startRefusal: null,
    opened: [],
  });
});
afterEach(() => vi.useRealTimers());

describe('About', () => {
  it('names the build, and says nothing of the working tree when it is clean', async () => {
    renderGated('/settings/about');
    expect(await screen.findByText('gglib 0.31.0 (abc1234)')).toBeInTheDocument();
    expect(screen.queryByText('Working tree modified since this build')).not.toBeInTheDocument();
  });

  it('says the working tree was modified since this build when the fingerprint is dirty', async () => {
    fake.version = { ...fake.version, fingerprint: 'abc1234-dirty' };
    renderGated('/settings/about');
    expect(await screen.findByText('Working tree modified since this build')).toBeInTheDocument();
  });

  it('opens the documentation', async () => {
    renderGated('/settings/about');
    await user.click(await screen.findByRole('button', { name: 'Documentation' }));
    expect(fake.opened).toEqual(['https://github.com/mmogr/gglib']);
  });

  it('stops gglib only when asked, then shows the screen once the daemon has gone', async () => {
    renderGated('/settings/about');
    await user.click(await screen.findByRole('button', { name: 'Stop gglib…' }));
    const asked = within(await screen.findByRole('dialog'));
    expect(asked.getByText('Stop gglib?')).toBeInTheDocument();
    await user.click(asked.getByRole('button', { name: 'Cancel' }));
    expect(fake.up).toBe(true);

    await user.click(screen.getByRole('button', { name: 'Stop gglib…' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Stop' }));
    expect(await screen.findByRole('button', { name: 'Stopping gglib…' })).toBeDisabled();
    expect(fake.up).toBe(false);

    await nextProbe();
    expect(await screen.findByText(LOST)).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'About' })).not.toBeInTheDocument();
  });

  it("says a refused stop in the daemon's words, and offers it again", async () => {
    fake.shutdown = async () => {
      throw new TransportError('CONFLICT', 'this server is not running as the gglib daemon');
    };
    renderGated('/settings/about');
    await user.click(await screen.findByRole('button', { name: 'Stop gglib…' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Stop' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('this server is not running as the gglib daemon');
    expect(screen.getByRole('button', { name: 'Stop gglib…' })).toBeEnabled();
    await nextProbe();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
  });
});

describe('the service-not-running screen', () => {
  it('on the desktop starts gglib, and gives the place back at its address once it answers', async () => {
    fake.desktop = true;
    fake.up = false;
    const router = renderGated('/settings/about');

    expect(await screen.findByText(DOWN)).toBeInTheDocument();
    expect(screen.getByText('sidebar').closest('aside')).toHaveAttribute('inert');
    await user.click(screen.getByRole('button', { name: 'Start gglib' }));
    expect(fake.starts).toBe(1);
    expect(screen.getByRole('button', { name: 'Starting gglib…' })).toBeDisabled();

    await nextProbe();
    expect(await screen.findByRole('button', { name: 'Stop gglib…' })).toBeInTheDocument();
    expect(screen.queryByText(DOWN)).not.toBeInTheDocument();
    expect(router.state.location.pathname).toBe('/settings/about');
  });

  it("says a refused start in the app's words, and offers it again", async () => {
    fake.desktop = true;
    fake.up = false;
    fake.startRefusal = 'port 9887 is held by another program (not a gglib daemon)';
    renderGated('/settings/about');

    await user.click(await screen.findByRole('button', { name: 'Start gglib' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('port 9887 is held by another program (not a gglib daemon)');
    expect(screen.getByRole('button', { name: 'Start gglib' })).toBeEnabled();
    expect(screen.getByText(DOWN)).toBeInTheDocument();
  });

  it('in a browser says it lost gglib, offers nothing to press, and goes when gglib answers', async () => {
    fake.up = false;
    renderGated('/settings/about');

    expect(await screen.findByText(LOST)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Start gglib' })).not.toBeInTheDocument();
    expect(screen.queryByText(DOWN)).not.toBeInTheDocument();
    await nextProbe();
    expect(screen.getByText(LOST)).toBeInTheDocument();

    fake.up = true;
    await nextProbe();
    await waitFor(() => expect(screen.queryByText(LOST)).not.toBeInTheDocument());
    expect(await screen.findByRole('button', { name: 'Stop gglib…' })).toBeInTheDocument();
  });

  it('is not shown before the daemon has first answered', async () => {
    fake.up = null;
    renderGated('/settings/about');
    expect(await screen.findByRole('button', { name: 'Stop gglib…' })).toBeInTheDocument();
    await nextProbe();
    expect(screen.queryByText(LOST)).not.toBeInTheDocument();
  });
});
