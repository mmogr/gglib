/**
 * The tray popover while the daemon comes and goes: no daemon is its own
 * state, with Start gglib disabled and no Turn on (TM-8); the poll hearing
 * it again reads every registry afresh, so a model that died with the old
 * daemon is not drawn, and nothing is claimed until the proxy's read has
 * landed, on the first open and on every recovery; and the panel keeps one
 * `/api/events` stream through it all (TX-1).
 *
 * The daemon's answers are the transport's. The event stream is the real
 * shared manager over a fake `apiFetch` in the one test that counts streams,
 * and a stub everywhere else.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({
  getProxyStatus: vi.fn(async (): Promise<unknown> => ({})),
  listServers: vi.fn(async (): Promise<unknown[]> => []),
  getRemoteStatus: vi.fn(async (): Promise<unknown> => ({})),
  revealSecret: vi.fn(async (): Promise<string | null> => null),
}));

/** The fake `/api/events`: up or not, and every stream it handed out. */
const events = vi.hoisted(() => ({
  real: false,
  up: true,
  streams: [] as AbortSignal[],
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  const sse = await vi.importActual<typeof import('../../../src/services/transport/events/sse')>('../../../src/services/transport/events/sse');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      // This fake daemon takes no token and no key: its health probe answers
      // exactly when its proxy status does.
      probeDaemon: () => transport.getProxyStatus().then(() => true, () => false),
      subscribe: events.real ? sse.subscribeSseEvent : vi.fn(() => () => {}),
      onEventStreamOpen: vi.fn(() => () => {}),
      reconnectEventStreamNow: events.real ? sse.reconnectEventStreamNow : vi.fn(),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport/api/client');
  return {
    ...actual,
    apiFetch: async (_path: string, init: { signal?: AbortSignal } = {}) => {
      if (!events.up) throw new Error('fetch failed');
      const signal = init.signal as AbortSignal;
      events.streams.push(signal);
      // A stream that stays open until its signal fires, as the daemon's does.
      const body = new ReadableStream<Uint8Array>({
        start(controller) {
          signal.addEventListener('abort', () => controller.error(new DOMException('aborted', 'AbortError')));
        },
      });
      return new Response(body, { status: 200 });
    },
  };
});
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: () => () => {},
}));

import { TrayPanel } from '../../../src/tray/TrayPanel';
import { RESIDENCY_WORDS } from '../../../src/components/status/words';
import { IDLE_STATUS } from '../../../src/services/remoteRegistry';
import { forgetServerList } from '../../../src/services/serverEvents';
import type { ProxyStatus } from '../../../src/services/transport/types/proxy';

const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null, pinned_to: null };
const DOWN: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null };
const POLL_MS = 2000;
const NOT_RUNNING = "gglib isn't running, so nothing can be served. Start it from the tray menu.";

const gone = async () => {
  throw new Error('fetch failed');
};

/** One poll of the probe, and what it set going. */
async function poll() {
  // Async, so each reconnect wait's promise settles before the next timer.
  await act(async () => {
    await vi.advanceTimersByTimeAsync(POLL_MS);
  });
}

describe('the tray panel while the daemon comes and goes', () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    for (const mock of Object.values(transport)) mock.mockReset();
    transport.listServers.mockImplementation(async () => []);
    transport.getRemoteStatus.mockImplementation(async () => IDLE_STATUS);
    transport.revealSecret.mockImplementation(async () => null);
    forgetServerList();
    Object.assign(events, { real: false, up: true, streams: [] });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  describe('with no daemon answering (TM-8)', () => {
    beforeEach(() => {
      transport.getProxyStatus.mockImplementation(gone);
    });

    it('says gglib is not running, not that the endpoint is off', async () => {
      render(<TrayPanel />);
      expect(await screen.findByText(NOT_RUNNING)).toBeInTheDocument();
      expect(screen.queryByText("Off. Clients can't connect.")).not.toBeInTheDocument();
      expect(screen.queryByRole('region', { name: 'Endpoint' })).not.toBeInTheDocument();
    });

    it('draws Start gglib disabled, and no Turn on', async () => {
      render(<TrayPanel />);
      const start = await screen.findByRole('button', { name: 'Start gglib' });
      expect(start).toBeDisabled();
      expect(screen.queryByRole('button', { name: 'Turn on' })).not.toBeInTheDocument();
    });

    it('recovers when the poll hears the daemon, and reads it afresh', async () => {
      render(<TrayPanel />);
      await screen.findByRole('button', { name: 'Start gglib' });
      transport.getProxyStatus.mockImplementation(async () => UP);
      await poll();
      expect(await screen.findByText('http://127.0.0.1:8080/v1')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Turn off' })).toBeEnabled();
      expect(screen.queryByRole('button', { name: 'Start gglib' })).not.toBeInTheDocument();
    });
  });

  it('before the first answer, claims neither, and offers neither Turn on nor Turn off', () => {
    transport.getProxyStatus.mockImplementation(() => new Promise(() => {}));
    render(<TrayPanel />);
    expect(screen.queryByText(/isn't running/)).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Endpoint' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Turn (on|off)/ })).not.toBeInTheDocument();
  });

  it('claims nothing between the poll hearing the daemon and the registries hearing it', async () => {
    // The probe answers; the bridge's own read does not.
    transport.getProxyStatus.mockImplementationOnce(async () => UP).mockImplementation(() => new Promise(() => {}));
    render(<TrayPanel />);
    await waitFor(() => expect(transport.getProxyStatus).toHaveBeenCalledTimes(2));
    await act(async () => {});
    expect(screen.queryByText("Off. Clients can't connect.")).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Endpoint' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Turn (on|off)/ })).not.toBeInTheDocument();
  });

  it('claims nothing on a recovery either, until the re-read has landed', async () => {
    transport.getProxyStatus.mockImplementation(async () => UP);
    render(<TrayPanel />);
    await screen.findByText('http://127.0.0.1:8080/v1');

    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);

    // Back: the probe answers, the re-read it sets going is held.
    transport.getProxyStatus.mockImplementationOnce(async () => UP).mockImplementation(() => new Promise(() => {}));
    await poll();
    await waitFor(() => expect(screen.queryByText(NOT_RUNNING)).not.toBeInTheDocument());
    expect(screen.queryByText("Off. Clients can't connect.")).not.toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Endpoint' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Turn (on|off)/ })).not.toBeInTheDocument();
  });

  it('a model that died with the old daemon is not drawn once the new one has answered', async () => {
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    transport.listServers.mockImplementation(async () => [{ model_id: 3, model_name: 'Phi-4', port: 9003 }]);
    render(<TrayPanel />);
    const loaded = await screen.findByRole('region', { name: 'Loaded' });
    expect(within(loaded).getByRole('listitem')).toHaveTextContent(`Phi-4${RESIDENCY_WORDS.loaded}`);

    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);

    transport.getProxyStatus.mockImplementation(async () => DOWN);
    transport.listServers.mockImplementation(async () => []);
    await poll();
    expect(await screen.findByText('Nothing loaded.')).toBeInTheDocument();
    expect(screen.queryByText('Phi-4')).not.toBeInTheDocument();
    expect(transport.listServers).toHaveBeenCalledTimes(2);
  });

  it('draws no model from the old daemon while the new one\'s list is still on its way', async () => {
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    transport.listServers.mockImplementation(async () => [{ model_id: 3, model_name: 'Phi-4', port: 9003 }]);
    render(<TrayPanel />);
    await screen.findByText('Phi-4');

    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);

    transport.getProxyStatus.mockImplementation(async () => DOWN);
    transport.listServers.mockImplementation(() => new Promise(() => {}));
    await poll();
    await screen.findByRole('region', { name: 'Endpoint' });
    await waitFor(() => expect(transport.listServers).toHaveBeenCalledTimes(2));
    expect(screen.queryByText('Phi-4')).not.toBeInTheDocument();
    expect(screen.queryByText(/Nothing loaded/)).not.toBeInTheDocument();
  });

  it('keeps one /api/events stream through an outage and back (TX-1)', async () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);
    Object.assign(events, { real: true, up: false });
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    const open = () => events.streams.filter((signal) => !signal.aborted).length;

    // The stream cannot connect while the status still answers, so its
    // reconnect waits grow (0.5 s, 1 s, 2 s, 4 s) …
    render(<TrayPanel />);
    await screen.findByRole('region', { name: 'Endpoint' });
    await poll();
    // … then the daemon is gone for a poll, while that loop sleeps …
    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);
    // … and back, before the sleeping loop wakes.
    events.up = true;
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    await poll();
    await screen.findByRole('region', { name: 'Endpoint' });
    for (let i = 0; i < 6; i++) await poll();

    expect(open()).toBe(1);
  });

  it('after a long outage, connects the stream as soon as the daemon answers, not after its reconnect wait', async () => {
    vi.spyOn(Math, 'random').mockReturnValue(0);
    Object.assign(events, { real: true, up: false });
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    const open = () => events.streams.filter((signal) => !signal.aborted).length;

    // Ten polls of a stream that cannot connect: its waits have grown to
    // 16 s (0.5 + 1 + 2 + 4 + 8 = 15.5 s, then 16 s more), so it sleeps on.
    render(<TrayPanel />);
    await screen.findByRole('region', { name: 'Endpoint' });
    for (let i = 0; i < 10; i++) await poll();
    expect(open()).toBe(0);

    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);

    // Back, eleven seconds before that wait would end.
    events.up = true;
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    await poll();
    await screen.findByRole('region', { name: 'Endpoint' });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(open()).toBe(1);
  });

  it('reads the list again each poll until one read lands, and only then says what is loaded', async () => {
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    transport.listServers.mockRejectedValueOnce(new Error('list refused')).mockImplementation(async () => []);
    render(<TrayPanel />);
    await screen.findByRole('region', { name: 'Endpoint' });
    await waitFor(() => expect(transport.listServers).toHaveBeenCalledTimes(1));
    expect(screen.queryByText(/Nothing loaded/)).not.toBeInTheDocument();

    await poll();
    expect(await screen.findByText('Nothing loaded.')).toBeInTheDocument();
    expect(transport.listServers).toHaveBeenCalledTimes(2);
    // Heard: no more reads for the list's sake.
    await poll();
    await poll();
    expect(transport.listServers).toHaveBeenCalledTimes(2);
  });

  it('reads the remote status again on a recovery, so Devices comes back with the daemon', async () => {
    transport.getProxyStatus.mockImplementation(async () => DOWN);
    render(<TrayPanel />);
    await screen.findByRole('region', { name: 'Devices' });

    transport.getProxyStatus.mockImplementation(gone);
    await poll();
    await screen.findByText(NOT_RUNNING);
    expect(screen.queryByRole('region', { name: 'Devices' })).not.toBeInTheDocument();

    transport.getProxyStatus.mockImplementation(async () => DOWN);
    await poll();
    expect(await screen.findByRole('region', { name: 'Devices' })).toBeInTheDocument();
    expect(transport.getRemoteStatus).toHaveBeenCalledTimes(2);
  });
});
