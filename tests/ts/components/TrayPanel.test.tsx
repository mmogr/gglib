/**
 * The tray popover, over a fake transport and its real registries: the
 * Endpoint block in each state (On with the address, ⧉ and a one-second
 * Copied; Off with its dimmed line and Turn on; Turn off; Pinned; Key
 * required with Copy key), the Loaded rows with their words and × (which
 * asks the daemon whatever the server list believes, TX-12), Nothing
 * loaded only once known, the Devices lines, a failure under its block,
 * and Open gglib, the panel's one window action, mocked at the platform's
 * Tauri boundary (TM-9). The daemon coming and going is
 * `TrayPanelRecovery.test.tsx`'s.
 *
 * The daemon's answers are the transport's; the dashboard stream is played
 * by the test.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({
  getProxyStatus: vi.fn(async (): Promise<unknown> => ({})),
  listServers: vi.fn(async (): Promise<unknown[]> => []),
  getRemoteStatus: vi.fn(async (): Promise<unknown> => ({})),
  startProxy: vi.fn(async (_c?: unknown) => ({})),
  stopProxy: vi.fn(async () => {}),
  stopServer: vi.fn(async (_id: number) => {}),
  revealSecret: vi.fn(async (): Promise<string | null> => null),
}));
const dashboard = vi.hoisted(() => ({ send: null as ((s: unknown) => void) | null }));
const tauri = vi.hoisted(() => ({ invoke: vi.fn(async (_cmd: string) => undefined) }));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      subscribe: vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      reconnectEventStreamNow: vi.fn(),
    }),
  };
});
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (_h: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: tauri.invoke }));

import { TrayPanel } from '../../../src/tray/TrayPanel';
import { ENDPOINT_WORDS, REACH_WORDS, RESIDENCY_WORDS } from '../../../src/components/status/words';
import { IDLE_STATUS } from '../../../src/services/remoteRegistry';
import { forgetServerList } from '../../../src/services/serverEvents';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import type { DashboardSnapshot } from '../../../src/services/transport/types/dashboard';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import { activeConnection, dashboardSnapshot, residentSlot } from '../fixtures/dashboard';
import { device } from '../places/devicesFixtures';

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null };
const DOWN: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null };

const endpoint = () => screen.getByRole('region', { name: 'Endpoint' });
const loaded = () => screen.getByRole('region', { name: 'Loaded' });

/** Render with the daemon answering `status`, and wait until the panel has heard it. */
async function renderTray(status: ProxyStatus = UP) {
  transport.getProxyStatus.mockImplementation(async () => status);
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  // After the setup, which puts its own clipboard on the navigator.
  vi.spyOn(navigator.clipboard, 'writeText');
  const view = render(<TrayPanel />);
  await screen.findByRole('region', { name: 'Endpoint' });
  if (status.running) await screen.findByText(status.base_url ?? '');
  return { ...view, user };
}

/** Play one dashboard frame with these residents, once the panel has opened the stream. */
async function frame(slots: ReturnType<typeof residentSlot>[] = []) {
  await waitFor(() => expect(dashboard.send).not.toBeNull());
  const base = dashboardSnapshot();
  act(() => dashboard.send?.({ ...base, admission: { ...base.admission, slots } } satisfies DashboardSnapshot));
}

describe('the tray panel', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    for (const mock of Object.values(transport)) mock.mockReset();
    transport.listServers.mockImplementation(async () => []);
    transport.getRemoteStatus.mockImplementation(async () => IDLE_STATUS);
    transport.startProxy.mockImplementation(async () => ({}));
    transport.stopProxy.mockImplementation(async () => {});
    transport.stopServer.mockImplementation(async () => {});
    transport.revealSecret.mockImplementation(async () => null);
    tauri.invoke.mockClear();
    forgetServerList();
  });
  afterEach(() => {
    vi.useRealTimers();
    delete window.__TAURI_INTERNALS__;
  });

  describe('the endpoint', () => {
    it('on: says gglib, Endpoint On and the address the daemon gives', async () => {
      await renderTray();
      expect(screen.getByText('gglib')).toBeInTheDocument();
      expect(within(endpoint()).getByText(ENDPOINT_WORDS.on)).toBeInTheDocument();
      expect(within(endpoint()).getByText('http://127.0.0.1:8080/v1')).toBeInTheDocument();
      expect(screen.queryByText("Off. Clients can't connect.")).not.toBeInTheDocument();
    });

    it('⧉ copies the address and says Copied for one second', async () => {
      const { user } = await renderTray();
      await user.click(screen.getByRole('button', { name: 'Copy the endpoint address' }));
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('http://127.0.0.1:8080/v1');
      expect(await within(endpoint()).findByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(900));
      expect(within(endpoint()).getByText('Copied')).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(150));
      expect(within(endpoint()).queryByText('Copied')).not.toBeInTheDocument();
    });

    it('off: the bar dims to its sentence, and Turn on starts it naming nothing', async () => {
      const { user } = await renderTray(DOWN);
      expect(within(endpoint()).getByText(ENDPOINT_WORDS.off)).toBeInTheDocument();
      expect(within(endpoint()).getByText("Off. Clients can't connect.")).toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Copy the endpoint address' })).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Turn on' }));
      expect(transport.startProxy).toHaveBeenCalledWith({});
      expect(screen.queryByRole('button', { name: 'Turn off' })).not.toBeInTheDocument();
    });

    it('on: Turn off stops it', async () => {
      const { user } = await renderTray();
      expect(screen.queryByRole('button', { name: 'Turn on' })).not.toBeInTheDocument();
      await user.click(screen.getByRole('button', { name: 'Turn off' }));
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    });

    it('pinned: On · only the model', async () => {
      await renderTray({ ...UP, state: 'pinned', pinned_model: 'Qwen3-4B' });
      expect(within(endpoint()).getByText(`${ENDPOINT_WORDS.on} · only Qwen3-4B`)).toBeInTheDocument();
    });

    it('key required: says so under the address, and Copy key copies the key revealed for the stream', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y-5f3a');
      const { user } = await renderTray({ ...UP, key: 'required' });
      expect(within(endpoint()).getByText('Key required')).toBeInTheDocument();
      const copyKey = within(endpoint()).getByRole('button', { name: 'Copy key' });
      await waitFor(() => expect(copyKey).toBeEnabled());
      await user.click(copyKey);
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith('k3y-5f3a');
      expect(await within(endpoint()).findByText('Copied')).toBeInTheDocument();
    });

    it('a key copy that fails says why under the endpoint', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y-5f3a');
      const { user } = await renderTray({ ...UP, key: 'required' });
      vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('clipboard refused'));
      const copyKey = within(endpoint()).getByRole('button', { name: 'Copy key' });
      await waitFor(() => expect(copyKey).toBeEnabled());
      await user.click(copyKey);
      expect(await within(endpoint()).findByRole('alert')).toHaveTextContent('clipboard refused');
      expect(within(endpoint()).queryByText('Copied')).not.toBeInTheDocument();
    });

    it('a key copy that works clears the failure before it', async () => {
      transport.revealSecret.mockImplementation(async () => 'k3y-5f3a');
      const { user } = await renderTray({ ...UP, key: 'required' });
      vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(new Error('clipboard refused'));
      const copyKey = within(endpoint()).getByRole('button', { name: 'Copy key' });
      await waitFor(() => expect(copyKey).toBeEnabled());
      await user.click(copyKey);
      await within(endpoint()).findByRole('alert');
      await user.click(copyKey);
      expect(await within(endpoint()).findByText('Copied')).toBeInTheDocument();
      expect(within(endpoint()).queryByRole('alert')).not.toBeInTheDocument();
    });

    it('up before the status has said where (Connecting): the mark turns, and no address is drawn', async () => {
      transport.getProxyStatus.mockImplementation(async () => ({ ...UP, base_url: null }));
      render(<TrayPanel />);
      const region = await screen.findByRole('region', { name: 'Endpoint' });
      const mark = within(region).getByText(ENDPOINT_WORDS.on).closest('[data-kind]');
      expect(mark).toHaveAttribute('data-kind', 'changing');
      expect(screen.queryByRole('button', { name: 'Copy the endpoint address' })).not.toBeInTheDocument();
    });

    it('Turn on waits while its start is on its way: a second click sends nothing', async () => {
      let started: () => void = () => {};
      transport.startProxy.mockImplementation(() => new Promise((resolve) => (started = () => resolve({}))));
      const { user } = await renderTray(DOWN);
      const turnOn = screen.getByRole('button', { name: 'Turn on' });
      await user.click(turnOn);
      expect(turnOn).toBeDisabled();
      await user.click(turnOn);
      expect(transport.startProxy).toHaveBeenCalledTimes(1);
      await act(async () => started());
      await waitFor(() => expect(screen.getByRole('button', { name: 'Turn on' })).toBeEnabled());
    });

    it('Turn off waits while its stop is on its way', async () => {
      transport.stopProxy.mockImplementation(() => new Promise(() => {}));
      const { user } = await renderTray();
      const turnOff = screen.getByRole('button', { name: 'Turn off' });
      await user.click(turnOff);
      expect(turnOff).toBeDisabled();
      await user.click(turnOff);
      expect(transport.stopProxy).toHaveBeenCalledTimes(1);
    });

    it('no key needed: no key line', async () => {
      await renderTray();
      expect(screen.queryByText('Key required')).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'Copy key' })).not.toBeInTheDocument();
    });

    it('a failed turn on is said under the endpoint, in the daemon’s words', async () => {
      transport.startProxy.mockRejectedValueOnce(new Error('port 8080 is in use'));
      const { user } = await renderTray(DOWN);
      await user.click(screen.getByRole('button', { name: 'Turn on' }));
      expect(await within(endpoint()).findByRole('alert')).toHaveTextContent('port 8080 is in use');
    });

    it('a long failure is cut to fit, and an empty one still says something', async () => {
      transport.startProxy.mockRejectedValueOnce(new Error('x'.repeat(500))).mockRejectedValueOnce(new Error(''));
      const { user } = await renderTray(DOWN);
      await user.click(screen.getByRole('button', { name: 'Turn on' }));
      const cut = await within(endpoint()).findByRole('alert');
      expect(cut.textContent?.length ?? 0).toBeLessThan(200);
      expect(cut).toHaveTextContent(/…$/);
      await user.click(screen.getByRole('button', { name: 'Turn on' }));
      await waitFor(() => expect(within(endpoint()).getByRole('alert')).toHaveTextContent('No further detail was reported.'));
    });
  });

  describe('loaded', () => {
    it('one row per resident, each with its word and ×', async () => {
      await renderTray();
      await frame([residentSlot(1, 'Qwen3-4B', { launch: { ...residentSlot(1, 'Qwen3-4B').launch!, state: 'working' } }), residentSlot(2, 'Gemma-3-4B-It')]);
      const rows = within(loaded()).getAllByRole('listitem');
      expect(rows).toHaveLength(2);
      expect(rows[0]).toHaveTextContent('Qwen3-4B');
      expect(rows[0]).toHaveTextContent(RESIDENCY_WORDS.working);
      expect(rows[1]).toHaveTextContent('Gemma-3-4B-It');
      expect(rows[1]).toHaveTextContent(RESIDENCY_WORDS.loaded);
    });

    it('a prompt being read says Reading and how far, and a load in progress turns', async () => {
      await renderTray();
      const base = dashboardSnapshot();
      await waitFor(() => expect(dashboard.send).not.toBeNull());
      act(() =>
        dashboard.send?.({
          ...base,
          admission: { ...base.admission, slots: [residentSlot(1, 'Qwen3-4B'), residentSlot(2, 'Gemma-3-4B-It', { launch: { ...residentSlot(2, 'x').launch!, state: 'loading' } })] },
          active_connections: [activeConnection({ id: 'c1', model_name: 'Qwen3-4B', phase: 'processing_prompt', prompt_processed: 61, prompt_total: 100 })],
        } satisfies DashboardSnapshot),
      );
      const [reading, loading] = within(loaded()).getAllByRole('listitem');
      expect(reading).toHaveTextContent(`${RESIDENCY_WORDS.reading} 61%`);
      expect(reading.querySelector('[data-kind]')).toHaveAttribute('data-kind', 'on');
      expect(loading).toHaveTextContent(RESIDENCY_WORDS.loading);
      expect(loading.querySelector('[data-kind]')).toHaveAttribute('data-kind', 'changing');
    });

    it('× asks the daemon even when the server list has not heard of the model', async () => {
      const { user } = await renderTray();
      await frame([residentSlot(7, 'Qwen3-4B')]);
      await user.click(within(loaded()).getByRole('button', { name: 'Unload Qwen3-4B' }));
      expect(transport.stopServer).toHaveBeenCalledWith(7);
    });

    it('a failed unload is said under Loaded', async () => {
      transport.stopServer.mockRejectedValueOnce(new Error('the daemon refused'));
      const { user } = await renderTray();
      await frame([residentSlot(7, 'Qwen3-4B')]);
      await user.click(within(loaded()).getByRole('button', { name: 'Unload Qwen3-4B' }));
      expect(await within(loaded()).findByRole('alert')).toHaveTextContent('the daemon refused');
      expect(within(endpoint()).queryByRole('alert')).not.toBeInTheDocument();
    });

    it('an unload that works clears the failure before it', async () => {
      transport.stopServer.mockRejectedValueOnce(new Error('the daemon refused'));
      const { user } = await renderTray();
      await frame([residentSlot(7, 'Qwen3-4B')]);
      await user.click(within(loaded()).getByRole('button', { name: 'Unload Qwen3-4B' }));
      await within(loaded()).findByRole('alert');
      await user.click(within(loaded()).getByRole('button', { name: 'Unload Qwen3-4B' }));
      await waitFor(() => expect(transport.stopServer).toHaveBeenCalledTimes(2));
      await waitFor(() => expect(within(loaded()).queryByRole('alert')).not.toBeInTheDocument());
    });

    it('nothing loaded: one sentence, once the dashboard has said so', async () => {
      await renderTray();
      await waitFor(() => expect(dashboard.send).not.toBeNull());
      expect(screen.queryByText('Nothing loaded. A model loads on its first request.')).not.toBeInTheDocument();
      await frame([]);
      expect(within(loaded()).getByText('Nothing loaded. A model loads on its first request.')).toBeInTheDocument();
    });

    it('off with nothing in memory: "Nothing loaded." alone, since no request can load one', async () => {
      await renderTray(DOWN);
      const note = await within(await screen.findByRole('region', { name: 'Loaded' })).findByText('Nothing loaded.');
      expect(note).toBeInTheDocument();
      expect(screen.queryByText(/A model loads on its first request/)).not.toBeInTheDocument();
    });

    it('off: says nothing of what is loaded until the server list has answered', async () => {
      transport.listServers.mockImplementation(() => new Promise(() => {}));
      await renderTray(DOWN);
      await waitFor(() => expect(transport.listServers).toHaveBeenCalled());
      await act(async () => {});
      expect(screen.queryByRole('region', { name: 'Loaded' })).not.toBeInTheDocument();
      expect(screen.queryByText(/Nothing loaded/)).not.toBeInTheDocument();
    });

    it('with the endpoint off, the running servers are the rows, each Loaded', async () => {
      transport.listServers.mockImplementation(async () => [{ model_id: 3, model_name: 'Phi-4', port: 9003 }]);
      const { user } = await renderTray(DOWN);
      const row = await within(await screen.findByRole('region', { name: 'Loaded' })).findByRole('listitem');
      expect(row).toHaveTextContent('Phi-4');
      expect(row).toHaveTextContent(RESIDENCY_WORDS.loaded);
      await user.click(within(row).getByRole('button', { name: 'Unload Phi-4' }));
      expect(transport.stopServer).toHaveBeenCalledWith(3);
    });
  });

  describe('devices', () => {
    const DESK: RemoteStatus = {
      ...IDLE_STATUS,
      connected: { port: 9100, base_url: 'http://127.0.0.1:9100', ticket_fingerprint: '3ca82708b995', path: 'direct', away_for_s: null },
      reach: 'connected',
      paired_name: 'desk',
    };

    it('the joined machine with its reach, and sharing with how many are paired', async () => {
      transport.getRemoteStatus.mockImplementation(async () => ({ ...DESK, enabled: true, devices: [device(), device({ id: 'dev-2' })] }));
      await renderTray();
      const devices = await screen.findByRole('region', { name: 'Devices' });
      const [joined, sharing] = within(devices).getAllByRole('listitem');
      expect(joined).toHaveTextContent('desk');
      expect(joined).toHaveTextContent(REACH_WORDS.connected);
      expect(sharing).toHaveTextContent('Sharing');
      expect(sharing).toHaveTextContent('On · 2 paired');
    });

    it('away: the reach word the daemon gives, with its own mark', async () => {
      transport.getRemoteStatus.mockImplementation(async () => ({ ...DESK, reach: 'away' }));
      await renderTray();
      const devices = await screen.findByRole('region', { name: 'Devices' });
      const [joined] = within(devices).getAllByRole('listitem');
      expect(joined).toHaveTextContent(`desk${REACH_WORDS.away}`);
      expect(joined.querySelector('[data-kind]')).toHaveAttribute('data-kind', 'away');
    });

    it('sharing alone, without a joined machine', async () => {
      transport.getRemoteStatus.mockImplementation(async () => ({ ...IDLE_STATUS, enabled: true, devices: [device()] }));
      await renderTray();
      const devices = await screen.findByRole('region', { name: 'Devices' });
      expect(within(devices).getByRole('listitem')).toHaveTextContent('On · 1 paired');
    });

    it('sharing off is a fact: the block says Sharing Off, and no joined machine', async () => {
      await renderTray();
      const devices = await screen.findByRole('region', { name: 'Devices' });
      const lines = within(devices).getAllByRole('listitem');
      expect(lines).toHaveLength(1);
      expect(lines[0]).toHaveTextContent('SharingOff');
      expect(lines[0].querySelector('[data-kind]')).toHaveAttribute('data-kind', 'off');
    });

    it('not drawn before the first status', async () => {
      transport.getRemoteStatus.mockImplementation(() => new Promise(() => {}));
      await renderTray();
      expect(screen.queryByRole('region', { name: 'Devices' })).not.toBeInTheDocument();
    });
  });

  describe('Open gglib', () => {
    it('in the desktop app, asks for the main window through show_main_window', async () => {
      window.__TAURI_INTERNALS__ = { invoke: vi.fn(async () => undefined) as never };
      const { user } = await renderTray();
      await user.click(screen.getByRole('button', { name: 'Open gglib' }));
      await waitFor(() => expect(tauri.invoke).toHaveBeenCalledWith('show_main_window'));
    });

    it('on the web, where there is no tray, is not drawn', async () => {
      await renderTray();
      expect(screen.queryByRole('button', { name: 'Open gglib' })).not.toBeInTheDocument();
    });
  });
});
