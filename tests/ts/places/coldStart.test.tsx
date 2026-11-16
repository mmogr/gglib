/**
 * The window on a cold start (A3 of #1959): the app's own layout,
 * `AppContent`, with every store it starts, over a daemon that is not yet
 * listening when the page runs. Every first read is refused; the stream's
 * first connection fails too. Once the daemon answers, the stream opens and
 * every store the window reads is filled: the settings, the model library
 * and the built-in tools as well as the endpoint, the servers and the
 * remote. And a healthy start, counted: how many reads each store makes.
 *
 * The stream is the real shared manager over a fake `apiFetch`; the
 * daemon's answers are the transport's. The shell is a stand-in that draws
 * what the stores hold, as a reader that never asks again itself.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { RouterProvider } from 'react-router';
import { appSettings } from '../fixtures/settings';
import { guiModel } from '../fixtures/model';
import { queueSnapshot } from '../fixtures/downloads';

/** The fake daemon: answering or not, and how often each read was asked. */
const daemon = vi.hoisted(() => ({ up: false, reads: {} as Record<string, number>, streams: [] as AbortSignal[] }));

vi.mock('../../../src/services/transport', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  const sse = await vi.importActual<typeof import('../../../src/services/transport/events/sse')>(
    '../../../src/services/transport/events/sse',
  );
  const { IDLE_STATUS } = await vi.importActual<typeof import('../../../src/services/remoteRegistry')>(
    '../../../src/services/remoteRegistry',
  );
  /** A read that the daemon refuses until it listens, counted. */
  const read =
    <T,>(name: string, answer: () => T) =>
    async (): Promise<T> => {
      daemon.reads[name] = (daemon.reads[name] ?? 0) + 1;
      if (!daemon.up) throw new TypeError('Failed to fetch');
      return answer();
    };
  return {
    ...actual,
    getTransport: () => ({
      getSettings: read('settings', () => appSettings({ defaultContextSize: 8192 })),
      listModels: read('models', () => [guiModel({ id: 1, name: 'Qwen3-4B' })]),
      listBuiltinTools: read('builtin tools', () => [
        { name: 'get_current_time', description: 'The time', input_schema: { type: 'object', properties: {} } },
      ]),
      getProxyStatus: read('proxy', () => ({ running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null, pinned_model_id: null })),
      listServers: read('servers', () => []),
      getRemoteStatus: read('remote', () => IDLE_STATUS),
      getDownloadQueue: read('downloads', () => queueSnapshot()),
      probeDaemon: async () => daemon.up,
      tokenRefusal: async () => null,
      subscribe: sse.subscribeSseEvent,
      onEventStreamOpen: sse.onEventStreamOpen,
      onEventStreamClose: sse.onEventStreamClose,
      isEventStreamOpen: sse.isEventStreamOpen,
      reconnectEventStreamNow: sse.reconnectEventStreamNow,
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  apiFetch: async (_path: string, init: { signal?: AbortSignal } = {}) => {
    if (!daemon.up) throw new TypeError('Failed to fetch');
    const signal = init.signal as AbortSignal;
    daemon.streams.push(signal);
    // A stream that stays open until its signal fires, as the daemon's does.
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        signal.addEventListener('abort', () => controller.error(new DOMException('aborted', 'AbortError')));
      },
    });
    return new Response(body, { status: 200 });
  },
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: async () => ({ setupCompleted: true }),
}));
vi.mock('../../../src/sidebar/Sidebar', () => ({ Sidebar: () => <nav aria-label="gglib">sidebar</nav> }));
vi.mock('../../../src/app/Shell', async () => {
  const { useSettings } = await import('../../../src/hooks/useSettings');
  const { useModelLibrary } = await import('../../../src/services/modelRegistry');
  const { useProxyState } = await import('../../../src/services/proxyRegistry');
  const { useServerListHeard } = await import('../../../src/services/serverEvents');
  /** What the stores hold, drawn by a reader that never asks again itself. */
  function Shell() {
    const { settings } = useSettings();
    const { models } = useModelLibrary();
    const { status } = useProxyState();
    const serversHeard = useServerListHeard();
    return (
      <main aria-label="shell">
        <p data-testid="settings">{settings ? `context ${settings.defaultContextSize}` : 'no settings'}</p>
        <p data-testid="library">{models.map((m) => m.name).join(', ') || 'no models'}</p>
        <p data-testid="endpoint">{status ? `endpoint ${status.state}` : 'no endpoint'}</p>
        <p data-testid="servers">{serversHeard ? 'servers read' : 'servers not read'}</p>
      </main>
    );
  }
  return { Shell };
});

import { AppContent } from '../../../src/App';
import { createAppRouter } from '../../../src/app/router';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { getToolRegistry, resetToolRegistry } from '../../../src/services/tools/registry';
import { forgetServerList } from '../../../src/services/serverEvents';

function renderWindow() {
  const router = createAppRouter(<AppContent />, ['/']);
  return render(
    <ToastProvider>
      <ConfirmProvider>
        <RouterProvider router={router} />
      </ConfirmProvider>
    </ToastProvider>,
  );
}

/** Let the probe's poll ask again, and whatever it set going run. */
const nextProbe = () => act(() => vi.advanceTimersByTimeAsync(2000));
const settle = () => act(() => vi.advanceTimersByTimeAsync(0));

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.spyOn(Math, 'random').mockReturnValue(0);
  Object.assign(daemon, { up: false, reads: {}, streams: [] });
  resetToolRegistry();
  forgetServerList();
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('the window on a cold start', () => {
  it('fills every store once the daemon answers, though each first read was refused', async () => {
    const view = renderWindow();
    expect(await screen.findByText('Lost gglib. Reconnecting…')).toBeInTheDocument();
    expect(daemon.reads.settings).toBe(1);
    expect(daemon.reads.models).toBe(1);
    expect(daemon.reads['builtin tools']).toBe(1);

    daemon.up = true;
    await nextProbe();
    await settle();

    expect(await screen.findByRole('main', { name: 'shell' })).toBeInTheDocument();
    // Soft, so a store left empty is named with every other one that is.
    expect.soft(screen.getByTestId('endpoint')).toHaveTextContent('endpoint off');
    expect.soft(screen.getByTestId('servers')).toHaveTextContent('servers read');
    expect.soft(screen.getByTestId('settings')).toHaveTextContent('context 8192');
    expect.soft(screen.getByTestId('library')).toHaveTextContent('Qwen3-4B');
    expect.soft(getToolRegistry().has('get_current_time'), 'the built-in tools registered').toBe(true);
    // Each store: the refused first read, and the one the stream's opening set going.
    expect(daemon.reads).toEqual({
      settings: 2,
      models: 2,
      'builtin tools': 2,
      proxy: 2,
      servers: 2,
      remote: 2,
      downloads: 2,
    });
    view.unmount();
  });
});

describe('the window after a long outage', () => {
  // The stream's reconnect wait has grown to 16 s; the probe hearing the
  // daemon wakes it, so the stores are filled within that probe's step.
  it('fills every store as soon as the probe hears the daemon, not after the stream\'s reconnect wait', async () => {
    const view = renderWindow();
    expect(await screen.findByText('Lost gglib. Reconnecting…')).toBeInTheDocument();
    // Ten polls: the stream's attempts at 0.5, 1.5, 3.5, 7.5 and 15.5 s refused; it sleeps until 31.5 s.
    for (let i = 0; i < 10; i++) await nextProbe();
    const streamsBefore = daemon.streams.length;

    daemon.up = true;
    await nextProbe();
    await settle();

    expect(daemon.streams).toHaveLength(streamsBefore + 1);
    expect(await screen.findByRole('main', { name: 'shell' })).toBeInTheDocument();
    expect.soft(screen.getByTestId('endpoint')).toHaveTextContent('endpoint off');
    expect.soft(screen.getByTestId('settings')).toHaveTextContent('context 8192');
    expect.soft(screen.getByTestId('library')).toHaveTextContent('Qwen3-4B');
    expect.soft(getToolRegistry().has('get_current_time'), 'the built-in tools registered').toBe(true);
    view.unmount();
  });
});

describe('the window on a healthy start', () => {
  // Each store reads now, and again when the stream first opens, since a
  // read answered before the stream was open may have missed an event.
  it('reads each store as many times as recorded here, and opens one stream', async () => {
    daemon.up = true;
    const view = renderWindow();
    expect(await screen.findByRole('main', { name: 'shell' })).toBeInTheDocument();
    await settle();
    await settle();

    expect(screen.getByTestId('settings')).toHaveTextContent('context 8192');
    expect(screen.getByTestId('library')).toHaveTextContent('Qwen3-4B');
    expect(daemon.reads).toEqual({
      settings: 2,
      models: 2,
      'builtin tools': 2,
      proxy: 2,
      servers: 2,
      remote: 2,
      downloads: 2,
    });
    expect(daemon.streams.filter((signal) => !signal.aborted)).toHaveLength(1);
    view.unmount();
  });
});
