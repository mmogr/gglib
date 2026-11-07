/**
 * The library place, driven by its address and by clicks through the real
 * routes and registries: groups of rows in their order, a model in one of
 * them, the words beside a row from the daemon's own fields, the search and
 * the sort, the paired machine's group, the empty, failed and loading
 * states, downloads as rows, and the llama.cpp banner. The search's way to
 * Hugging Face is `librarySearch.test.tsx`.
 *
 * The transport is the fake; the event stream is played through the
 * handlers the registries subscribe with.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[], fail: null as string | null, hang: false }));
const servers = vi.hoisted(() => ({ list: [] as unknown[] }));
const downloads = vi.hoisted(() => ({ queue: null as unknown }));
const setup = vi.hoisted(() => ({ status: { llamaInstalled: true, llamaCanDownload: true } as Record<string, unknown> }));
const handlers = vi.hoisted(() => new Map<string, (evt: unknown) => void>());
const transport = vi.hoisted(() => ({
  listModels: vi.fn(),
  listServers: vi.fn(),
  stopServer: vi.fn(async (_id: number) => {}),
  getDownloadQueue: vi.fn(),
  cancelDownload: vi.fn(async (_id: string) => {}),
  removeFromQueue: vi.fn(async (_id: string) => {}),
  listPairedModels: vi.fn(),
}));
// The proxy's live dashboard: the test sends its frames.
const dashboard = vi.hoisted(() => ({ send: null as ((snapshot: unknown) => void) | null, host: null as string | null }));
const install = vi.hoisted(() => ({
  stream: vi.fn((_kind: string, _onEvent: (e: unknown) => void, _onError: (m: string) => void) => () => {}),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      getModelDetail: vi.fn(async () => null),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn((category: string, handler: (evt: unknown) => void) => {
        handlers.set(category, handler);
        return () => handlers.delete(category);
      }),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => setup.status),
  streamEngineInstall: install.stream,
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));
vi.mock('../../../src/services/clients/proxyDashboard', () => ({
  subscribeProxyDashboard: (host: string, _p: number, _k: unknown, onSnapshot: (s: unknown) => void) => {
    dashboard.host = host;
    dashboard.send = onSnapshot;
    return () => {
      dashboard.send = null;
    };
  },
}));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { cleanupDownloadEvents, initDownloadEvents } from '../../../src/services/downloadsEvents';
import { cleanupServerEvents, initServerEvents } from '../../../src/services/serverEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
import { applyProxyStatus, ingestProxyEvent, resetProxyState } from '../../../src/services/proxyRegistry';
import { resetEngineInstall } from '../../../src/places/library/engineInstall';
import { INSTALL_PHASE_LABELS } from '../../../src/types/setup';
import { getSetupStatus } from '../../../src/services/transport/api/setup';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { RemoteStatus } from '../../../src/services/transport/types/remote';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';
import { farEntry, pairedModels } from '../fixtures/fakeFarDaemon';
import { queueSnapshot, runningRow, waitingRow } from '../fixtures/downloads';

/**
 * A clock past every write an earlier test left in the server registry,
 * whose writes are ordered by time: each call is later than the last.
 */
let clock = Date.now() + 1e9;
const later = () => (clock += 1e6);

/** A server list row for model `id`, started after anything an earlier test left. */
const listed = (id: number, name: string, extra: Record<string, unknown> = {}) => ({
  model_id: id, model_name: name, pid: 1, port: 9000 + id, started_at: Math.floor(later() / 1000), runtime: 'llama', ...extra,
});

/** A dashboard frame whose admission slots hold `residents`, each a model id and its state. */
const frame = (...residents: [number, string][]) => ({
  admission: {
    slots: residents.map(([model_id, state]) => ({
      model_id,
      launch: { state, context_size: 8192, weights_bytes: 1, quantization: null, narration: null, flag_line: '' },
    })),
  },
});

const DESK: RemoteStatus = {
  ...IDLE_STATUS,
  connected: { port: 41234, base_url: 'http://127.0.0.1:41234/v1', ticket_fingerprint: '3ca82708b995', path: 'direct', away_for_s: null },
  reach: 'connected',
  stored_ticket_fingerprint: '3ca82708b995',
  paired_name: 'desk',
};

function renderLibrary(path = '/library') {
  initModelEvents();
  initDownloadEvents();
  initServerEvents();
  return renderApp(path);
}

/** The names of the library's groups, in the order drawn. */
const groupNames = () => screen.getAllByRole('region').map((r) => r.getAttribute('aria-label')).filter((n) => n !== 'Library');

/** The names of the rows in the group called `name`. */
const rowsOf = (name: string) =>
  within(screen.getByRole('listbox', { name })).getAllByRole('option').map((o) => o.firstElementChild?.firstElementChild?.textContent);

describe('the library place', () => {
  beforeEach(() => {
    library.models = [];
    library.fail = null;
    library.hang = false;
    servers.list = [];
    downloads.queue = queueSnapshot();
    setup.status = { llamaInstalled: true, llamaCanDownload: true };
    for (const mock of Object.values(transport)) mock.mockClear();
    install.stream.mockClear();
    transport.listModels.mockImplementation(() =>
      library.hang ? new Promise(() => {}) : library.fail ? Promise.reject(new Error(library.fail)) : Promise.resolve(library.models),
    );
    transport.listServers.mockImplementation(async () => servers.list);
    transport.getDownloadQueue.mockImplementation(async () => downloads.queue);
    transport.listPairedModels.mockResolvedValue(pairedModels([farEntry('Llama-3.3-70B', 7, { quantization: 'Q4_K_M', capabilities: ['vision'] })]));
    resetRemoteState();
    resetEngineInstall();
  });
  afterEach(() => {
    act(() => resetProxyState());
    cleanupModelEvents();
    cleanupDownloadEvents();
    cleanupServerEvents();
    for (const id of [1, 2, 3]) act(() => ingestServerEvent({ type: 'stopped', modelId: String(id), updatedAt: later() }));
  });

  it('draws Downloading, Loaded, On this Mac and the paired machine in that order, a model in one group', async () => {
    library.models = [
      guiModel({ id: 1, name: 'Qwen3-4B' }),
      guiModel({ id: 2, name: 'Gemma-3-4B-It' }),
      guiModel({ id: 3, name: 'Qwen3-30B-A3B' }),
    ];
    servers.list = [listed(2, 'Gemma-3-4B-It')];
    downloads.queue = queueSnapshot({ revision: 2, active: runningRow() });
    applyRemoteStatus(DESK);
    renderLibrary();

    await screen.findByRole('listbox', { name: 'On desk' });
    await screen.findByRole('listbox', { name: 'Loaded' });
    expect(groupNames()).toEqual(['Downloading', 'Loaded', 'On this Mac', 'On desk']);
    expect(rowsOf('Loaded')).toEqual(['Gemma-3-4B-It']);
    expect(rowsOf('On this Mac')).toEqual(['Qwen3-4B', 'Qwen3-30B-A3B']);
  });

  it("says a loaded model's state as the endpoint's live stream gives it, and Loaded while it says nothing", async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    servers.list = [listed(1, 'Qwen3-4B')];
    act(() => ingestProxyEvent({ type: 'proxy_started', port: 8080 }));
    renderLibrary();
    const row = await screen.findByRole('option', { name: /Qwen3-4B/ });
    await waitFor(() => expect(within(row).getByText('Loaded')).toBeInTheDocument());
    expect(within(screen.getByRole('listbox', { name: 'Loaded' })).getByRole('option')).toBe(row);
    await waitFor(() => expect(dashboard.send).not.toBeNull());

    // A request starts, then ends: the word follows the resident's slot.
    act(() => dashboard.send?.(frame([1, 'working'])));
    expect(within(row).getByText('Working')).toBeInTheDocument();
    act(() => dashboard.send?.(frame([1, 'loaded'])));
    expect(within(row).getByText('Loaded')).toBeInTheDocument();
    // Another model's slot says nothing of this one.
    act(() => dashboard.send?.(frame([2, 'working'])));
    expect(within(row).getByText('Loaded')).toBeInTheDocument();
  });

  it('reaches the live stream where the daemon says the endpoint is, not always on loopback', async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    servers.list = [listed(1, 'Qwen3-4B')];
    act(() =>
      applyProxyStatus({
        running: true,
        state: 'on',
        port: 8080,
        base_url: 'http://192.168.1.5:8080/v1',
        key: 'none_needed',
        pinned_model: null,
      }),
    );
    renderLibrary();
    await waitFor(() => expect(dashboard.send).not.toBeNull());
    expect(dashboard.host).toBe('192.168.1.5');
  });

  it("says the facts under a model's name, and a missing part as the one accessory", async () => {
    library.models = [
      guiModel({ id: 1, name: 'Qwen3-30B-A3B', paramCountB: 30.5, expertCount: 128, expertUsedCount: 8, quantization: 'Q4_K_M', sizeBytes: 18.6 * 1024 ** 3 }),
      guiModel({ id: 2, name: 'gemma', imageInput: true, sizeBytes: 0 }),
      guiModel({ id: 3, name: 'flux1-schnell', paramCountB: 12, imageFamily: 'flux1', missingComponents: ['vae'] }),
    ];
    renderLibrary();

    expect(await screen.findByRole('option', { name: /Qwen3-30B-A3B/ })).toHaveTextContent('30.5B · 8/128 experts · Q4_K_M · 18.60 GiB');
    expect(screen.getByRole('option', { name: /gemma/ })).toHaveTextContent(/^gemma7\.0B · sees images$/);
    const flux = screen.getByRole('option', { name: /flux1-schnell/ });
    expect(flux).toHaveTextContent('draws (Flux.1)');
    expect(within(flux).getByText('1 part missing')).toBeInTheDocument();
  });

  it('filters as you type, and says when nothing matches', async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-8B', quantization: 'Q4_K_M' }), guiModel({ id: 2, name: 'gemma', imageInput: true })];
    renderLibrary();
    await screen.findByRole('option', { name: /gemma/ });
    const user = userEvent.setup();

    await user.type(screen.getByRole('searchbox', { name: 'Search your models, or Hugging Face' }), 'vision');
    expect(rowsOf('On this Mac')).toEqual(['gemma']);

    await user.type(screen.getByRole('searchbox'), ' q4');
    expect(screen.getByText('Nothing in your library matches. Press ↵ to search Hugging Face.')).toBeInTheDocument();
    expect(screen.queryByRole('listbox', { name: 'On this Mac' })).not.toBeInTheDocument();
  });

  it('sorts each group by what Sort picks', async () => {
    library.models = [guiModel({ id: 1, name: 'a-small', sizeBytes: 1 }), guiModel({ id: 2, name: 'b-big', sizeBytes: 9 })];
    renderLibrary();
    await screen.findByRole('option', { name: /a-small/ });
    const user = userEvent.setup();
    expect(rowsOf('On this Mac')).toEqual(['a-small', 'b-big']);

    await user.click(screen.getByRole('button', { name: 'Sort' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Size' }));
    await user.click(screen.getByRole('button', { name: 'Sort' }));
    await user.click(screen.getByRole('menuitemradio', { name: 'Descending' }));

    expect(rowsOf('On this Mac')).toEqual(['b-big', 'a-small']);
  });

  it("heads the paired machine's group with its name and the reach the remote status says", async () => {
    applyRemoteStatus(DESK);
    library.models = [guiModel({ id: 1, name: 'here' })];
    renderLibrary();

    const group = await screen.findByRole('region', { name: 'On desk' });
    expect(within(group).getByText('Connected')).toBeInTheDocument();
    expect(within(group).getByRole('option', { name: /Llama-3.3-70B/ })).toHaveTextContent(
      'Q4_K_M · sees images · qwen3 - 8B parameters, Q4_K_M',
    );

    act(() => applyRemoteStatus({ ...DESK, reach: 'away', connected: { ...DESK.connected!, away_for_s: 30 } }));
    expect(within(group).getByText('Away')).toBeInTheDocument();
    expect(within(group).queryByText(/reached|stale|away$/)).not.toBeInTheDocument();

    await userEvent.setup().type(screen.getByRole('searchbox'), 'mistral');
    expect(within(group).getByText("None of desk's models match.")).toBeInTheDocument();
  });

  it('says so with no models here, pointing at the paired machine when there is one', async () => {
    renderLibrary();
    expect(await screen.findByText('No models on this Mac.')).toBeInTheDocument();
    cleanup();
    cleanupModelEvents();

    applyRemoteStatus(DESK);
    renderLibrary();
    expect(await screen.findByText("No models on this Mac. desk's models below work as they are.")).toBeInTheDocument();
  });

  it("says why it can't read the library, and reads it again on Retry", async () => {
    library.fail = 'daemon away';
    renderLibrary();

    expect(await screen.findByText("Can't read the library: daemon away.")).toBeInTheDocument();
    library.fail = null;
    library.models = [guiModel({ id: 1, name: 'back' })];
    await userEvent.setup().click(screen.getByRole('button', { name: 'Retry' }));

    expect(await screen.findByRole('option', { name: /back/ })).toBeInTheDocument();
  });

  it('draws six still grey rows before the first read', async () => {
    library.hang = true;
    renderLibrary();

    expect(await screen.findByRole('status', { name: '' })).toHaveTextContent('Loading models…');
    expect(screen.queryByRole('option')).not.toBeInTheDocument();
  });

  it("draws downloads in their rows' own words, cancels the running one and takes a waiting one off", async () => {
    downloads.queue = queueSnapshot({ revision: 3, active: runningRow(), waiting: [waitingRow('owner/b:Q4_K_M', 2)] });
    renderLibrary();
    const user = userEvent.setup();

    const running = await screen.findByRole('group', { name: 'owner/zeta-GGUF:Q8_0' });
    expect(running).toHaveTextContent('25.0% · ETA 2m 40s');
    expect(within(running).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '25');
    expect(screen.getByRole('group', { name: 'owner/b:Q4_K_M' })).toHaveTextContent('Queued');

    await user.click(screen.getByRole('button', { name: 'Cancel owner/zeta-GGUF:Q8_0' }));
    expect(transport.cancelDownload).toHaveBeenCalledWith('owner/zeta-GGUF:Q8_0');
    await user.click(screen.getByRole('button', { name: 'Remove owner/b:Q4_K_M from the queue' }));
    expect(transport.removeFromQueue).toHaveBeenCalledWith('owner/b:Q4_K_M');

    // The queue moves on: a snapshot without them takes the rows away.
    act(() => handlers.get('download')?.({ type: 'download', event: { type: 'queue_snapshot', ...queueSnapshot({ revision: 9 }) } }));
    expect(screen.queryByRole('region', { name: 'Downloading' })).not.toBeInTheDocument();
  });

  it('picks a row by the address, changing only its fill, and opens the inspector beside it', async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    const { at } = renderLibrary();
    const row = await screen.findByRole('option', { name: /Qwen3-4B/ });
    expect(screen.queryByRole('complementary', { name: 'Inspector' })).not.toBeInTheDocument();
    const before = new Set(row.className.split(/\s+/));

    await userEvent.setup().click(row);

    expect(at()).toBe('/models/1');
    expect(row).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('complementary', { name: 'Inspector' })).toBeInTheDocument();
    // Only the fill changes: no border, no padding, nothing that moves text.
    const after = new Set(row.className.split(/\s+/));
    const changed = [...before, ...after].filter((c) => before.has(c) !== after.has(c));
    expect(changed.length).toBeGreaterThan(0);
    expect(changed.filter((c) => !c.includes('bg-'))).toEqual([]);
  });

  it("unloads a loaded row through the daemon, and copies a row's Model ID", async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B', openaiId: 'Qwen3-4B' })];
    servers.list = [listed(1, 'Qwen3-4B')];
    const { at } = renderLibrary();
    const user = userEvent.setup();
    const row = await screen.findByRole('option', { name: /Qwen3-4B/ });
    await waitFor(() => expect(within(row).getByText('Loaded')).toBeInTheDocument());

    await user.click(within(row).getByRole('button', { name: 'Unload' }));
    expect(transport.stopServer).toHaveBeenCalledWith(1);
    // Unload acts on the row; it does not pick it.
    expect(at()).toBe('/library');
    await user.click(within(row).getByRole('button', { name: 'Copy the Model ID Qwen3-4B' }));
    expect(await navigator.clipboard.readText()).toBe('Qwen3-4B');
    // Said in the row, beside ⧉, not as a toast.
    expect(await within(row).findByText('Copied')).toBeInTheDocument();
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
  });

  it('picks a row with Enter or Space as with a click', async () => {
    library.models = [guiModel({ id: 1, name: 'a' }), guiModel({ id: 2, name: 'b' })];
    const { at } = renderLibrary();
    const user = userEvent.setup();
    (await screen.findByRole('option', { name: /^a/ })).focus();

    await user.keyboard('{Enter}');
    expect(at()).toBe('/models/1');
    screen.getByRole('option', { name: /^b/ }).focus();
    await user.keyboard(' ');
    expect(at()).toBe('/models/2');
  });

  it('is one Tab stop per group, and ↑ ↓ Home End move the focus and the pick through it', async () => {
    library.models = [guiModel({ id: 1, name: 'a' }), guiModel({ id: 2, name: 'b' }), guiModel({ id: 3, name: 'c' })];
    const { at } = renderLibrary();
    const user = userEvent.setup();
    const a = await screen.findByRole('option', { name: /^a/ });
    const b = screen.getByRole('option', { name: /^b/ });
    const c = screen.getByRole('option', { name: /^c/ });
    expect(rowsOf('On this Mac')).toEqual(['a', 'b', 'c']);
    // None picked: the first row is the stop.
    expect([a, b, c].map((r) => r.tabIndex)).toEqual([0, -1, -1]);

    a.focus();
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(b);
    expect(at()).toBe('/models/2');
    expect(b).toHaveAttribute('aria-selected', 'true');
    expect([a, b, c].map((r) => r.tabIndex)).toEqual([-1, 0, -1]);

    await user.keyboard('{ArrowUp}{ArrowUp}');
    expect(document.activeElement).toBe(a);
    expect(at()).toBe('/models/1');

    await user.keyboard('{End}');
    expect(document.activeElement).toBe(c);
    expect(at()).toBe('/models/3');
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(c);
    await user.keyboard('{Home}');
    expect(document.activeElement).toBe(a);
    expect(at()).toBe('/models/1');
  });

  it('makes the row the address picks the Tab stop', async () => {
    library.models = [guiModel({ id: 1, name: 'a' }), guiModel({ id: 2, name: 'b' })];
    renderLibrary('/models/2');
    const a = await screen.findByRole('option', { name: /^a/ });
    expect([a, screen.getByRole('option', { name: /^b/ })].map((r) => r.tabIndex)).toEqual([-1, 0]);
  });

  it('filters the downloads by the search too', async () => {
    downloads.queue = queueSnapshot({ revision: 3, active: runningRow(), waiting: [waitingRow('owner/b:Q4_K_M', 2)] });
    renderLibrary();
    await screen.findByRole('group', { name: 'owner/b:Q4_K_M' });

    await userEvent.setup().type(screen.getByRole('searchbox'), 'zeta');

    expect(screen.getByRole('group', { name: 'owner/zeta-GGUF:Q8_0' })).toBeInTheDocument();
    expect(screen.queryByRole('group', { name: 'owner/b:Q4_K_M' })).not.toBeInTheDocument();
  });

  describe('the llama.cpp banner', () => {
    const older = { llamaInstalled: true, llamaCanDownload: true, llamaPin: { standing: 'older', installedRelease: 'b6900', targetRelease: 'b7012', built: false, rebuilds: false, updateDownloads: true } };

    /** Away from the library and back, which unmounts the banner and mounts it again. */
    function awayAndBack(view: { unmount: () => void }) {
      view.unmount();
      expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument();
      renderApp('/library');
    }

    it('offers the update with a model running on llama.cpp, runs the install stream, and shows the refusal it answers with', async () => {
      setup.status = { ...older, llamaServing: 'qwen' };
      renderLibrary();
      const banner = await screen.findByRole('status', { name: 'llama.cpp' });
      expect(banner).toHaveTextContent('llama.cpp b6900 is older than the b7012 this gglib expects.');

      await userEvent.setup().click(within(banner).getByRole('button', { name: 'Update' }));
      expect(install.stream).toHaveBeenCalledWith('llama', expect.any(Function), expect.any(Function), expect.any(Function));
      const refuse = install.stream.mock.calls[0][2];
      act(() => refuse('qwen is running on llama-server. Stop it before installing llama.cpp.'));

      expect(banner).toHaveTextContent('qwen is running on llama-server. Stop it before installing llama.cpp.');
    });

    it('offers an install when there is none, and goes on Not now', async () => {
      setup.status = { llamaInstalled: false, llamaCanDownload: true };
      const view = renderLibrary();
      const banner = await screen.findByRole('status', { name: 'llama.cpp' });
      expect(banner).toHaveTextContent('llama.cpp is not installed.');
      expect(within(banner).getByRole('button', { name: 'Install' })).toBeInTheDocument();

      await userEvent.setup().click(within(banner).getByRole('button', { name: 'Not now' }));
      expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument();

      // Leaving the library and coming back does not bring it back.
      awayAndBack(view);
      await screen.findByRole('searchbox');
      await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
      expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument();
    });

    it('keeps a running install, and its progress, across leaving the library', async () => {
      setup.status = older;
      const view = renderLibrary();
      await userEvent.setup().click(await screen.findByRole('button', { name: 'Update' }));
      const send = install.stream.mock.calls[0][1];
      act(() => send({ type: 'phase_started', phase: 'download' }));

      awayAndBack(view);

      const banner = await screen.findByRole('status', { name: 'llama.cpp' });
      expect(within(banner).getByText(INSTALL_PHASE_LABELS.download)).toBeInTheDocument();
      expect(within(banner).queryByRole('button', { name: 'Update' })).not.toBeInTheDocument();
      expect(install.stream).toHaveBeenCalledTimes(1);
    });

    it('reads the status again when the install completes, and goes once llama.cpp is at its release', async () => {
      setup.status = older;
      renderLibrary();
      await userEvent.setup().click(await screen.findByRole('button', { name: 'Update' }));
      const reads = vi.mocked(getSetupStatus).mock.calls.length;
      setup.status = { llamaInstalled: true, llamaCanDownload: true };

      act(() => install.stream.mock.calls[0][1]({ type: 'completed', version: 'b7012' }));

      await waitFor(() => expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument());
      expect(vi.mocked(getSetupStatus).mock.calls.length).toBe(reads + 1);
    });

    it('is not drawn when llama.cpp is at its release', async () => {
      library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
      renderLibrary();
      await screen.findByRole('option', { name: /Qwen3-4B/ });
      await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
      expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument();
    });
  });
});
