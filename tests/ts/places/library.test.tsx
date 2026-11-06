/**
 * The library place, driven by its address and by clicks through the real
 * routes and registries: groups of rows in their order, a model in one of
 * them, the words beside a row from the daemon's own fields, the search and
 * the sort, the paired machine's group, the empty, failed and loading
 * states, downloads as rows, and the llama.cpp banner.
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
const install = vi.hoisted(() => ({
  stream: vi.fn((_kind: string, _onEvent: (e: unknown) => void, _onError: (m: string) => void) => () => {}),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({
      ...transport,
      listTags: vi.fn(async () => []),
      getModelDetail: vi.fn(async () => null),
      getSettings: vi.fn(async () => ({})),
      subscribe: vi.fn((category: string, handler: (evt: unknown) => void) => {
        handlers.set(category, handler);
        return () => handlers.delete(category);
      }),
      onEventStreamOpen: vi.fn(() => () => {}),
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({ get: vi.fn(async () => null) }));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => setup.status),
  streamEngineInstall: install.stream,
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { cleanupDownloadEvents, initDownloadEvents } from '../../../src/services/downloadsEvents';
import { cleanupServerEvents, initServerEvents } from '../../../src/services/serverEvents';
import { ingestServerEvent } from '../../../src/services/serverRegistry';
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

const launch = (state: string) => ({ state, context_size: 8192, weights_bytes: 1, quantization: null, narration: null, flag_line: '' });

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
  });
  afterEach(() => {
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

  it("says a loaded model's state in the word for the launch state the server list gives", async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    renderLibrary();
    await screen.findByRole('option', { name: /Qwen3-4B/ });

    // A load is announced by event; what it is doing is on the list it
    // re-reads, dated by the server's start, before the event arrived.
    servers.list = [listed(1, 'Qwen3-4B', { started_at: 1, launch: launch('working') })];
    vi.spyOn(Date, 'now').mockReturnValueOnce(later());
    act(() => handlers.get('server')?.({ type: 'server_started', modelId: 1, modelName: 'Qwen3-4B', port: 9001 }));

    const row = await screen.findByRole('option', { name: /Qwen3-4B/ });
    await waitFor(() => expect(within(row).getByText('Working')).toBeInTheDocument());
    expect(within(screen.getByRole('listbox', { name: 'Loaded' })).getByRole('option')).toBe(row);
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

    await user.type(screen.getByRole('searchbox', { name: 'Search your models' }), 'vision');
    expect(rowsOf('On this Mac')).toEqual(['gemma']);

    await user.type(screen.getByRole('searchbox'), ' q4');
    expect(screen.getByText('Nothing in your library matches.')).toBeInTheDocument();
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
    servers.list = [listed(1, 'Qwen3-4B', { launch: launch('loaded') })];
    renderLibrary();
    const user = userEvent.setup();
    const row = await screen.findByRole('option', { name: /Qwen3-4B/ });
    await waitFor(() => expect(within(row).getByText('Loaded')).toBeInTheDocument());

    await user.click(within(row).getByRole('button', { name: 'Unload' }));
    expect(transport.stopServer).toHaveBeenCalledWith(1);
    await user.click(within(row).getByRole('button', { name: 'Copy the Model ID Qwen3-4B' }));
    expect(await navigator.clipboard.readText()).toBe('Qwen3-4B');
  });

  it('shows the Add pane at its address in place of the rows, with the way back', async () => {
    library.models = [guiModel({ id: 1, name: 'Qwen3-4B' })];
    const { at } = renderLibrary();
    const user = userEvent.setup();
    await screen.findByRole('option', { name: /Qwen3-4B/ });

    await user.click(screen.getByRole('button', { name: 'Add' }));
    await user.click(screen.getByRole('menuitem', { name: 'Add a GGUF file…' }));

    expect(at()).toBe('/library?view=add&sub=add');
    expect(screen.queryByRole('option')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Your models' }));
    expect(at()).toBe('/library');
    expect(screen.getByRole('option', { name: /Qwen3-4B/ })).toBeInTheDocument();
  });

  describe('the llama.cpp banner', () => {
    const older = { llamaInstalled: true, llamaCanDownload: true, llamaPin: { standing: 'older', installedRelease: 'b6900', targetRelease: 'b7012', built: false, rebuilds: false, updateDownloads: true } };

    it('offers the update, runs the install stream, and shows the refusal it answers with', async () => {
      setup.status = older;
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
      renderLibrary();
      const banner = await screen.findByRole('status', { name: 'llama.cpp' });
      expect(banner).toHaveTextContent('llama.cpp is not installed.');
      expect(within(banner).getByRole('button', { name: 'Install' })).toBeInTheDocument();

      await userEvent.setup().click(within(banner).getByRole('button', { name: 'Not now' }));
      expect(screen.queryByRole('status', { name: 'llama.cpp' })).not.toBeInTheDocument();
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
