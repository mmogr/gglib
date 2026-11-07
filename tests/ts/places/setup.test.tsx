/**
 * Setup, behind the gate, driven through the real routes: the gate only
 * while the daemon says setup is not done; This Mac's facts and folder
 * Change; First model's suggestion in the route's own words, Get it with
 * its row's progress, Skip, Choose another and Join; Endpoint turned on,
 * its proof waiting on the download without holding anyone, and the
 * autostart setting by its wire name; Connect with the Model ID filled in
 * and Open gglib landing on the new model; "Already done" after `gglib up`;
 * and "Can't reach gglib." with Try again. The engines step has its own
 * file (`setupEngines.test.tsx`).
 *
 * The transport is a fake; the registries are the real ones, fed by it.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const queue = vi.hoisted(() => ({ snapshot: null as unknown }));
const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => library.models),
  listServers: vi.fn(async () => [] as unknown[]),
  getDownloadQueue: vi.fn(async () => queue.snapshot),
  queueDownload: vi.fn(async (_p: { modelId: string; quantization?: string }) => ({ id: 'unsloth/Qwen3-8B-GGUF' })),
  startProxy: vi.fn(async (_c?: unknown): Promise<unknown> => ({})),
  proveProxy: vi.fn(async (_id: number | null): Promise<unknown> => ({})),
  getProxyClients: vi.fn(async (_id: number | null): Promise<unknown> => null),
  revealSecret: vi.fn(async (): Promise<string | null> => null),
  getSettings: vi.fn(async (): Promise<Record<string, unknown>> => ({ proxyAutostart: false })),
  updateSettings: vi.fn(async (u: Record<string, unknown>) => u),
  getModelsDirectory: vi.fn(async (): Promise<unknown> => ({})),
  setModelsDirectory: vi.fn(async (_p: string) => {}),
  joinRemote: vi.fn(async (_b: unknown): Promise<unknown> => ({})),
}));
const api = vi.hoisted(() => ({
  getSetupStatus: vi.fn(async (): Promise<unknown> => ({})),
  getEngineStatus: vi.fn(async (_k: string): Promise<unknown> => ({})),
  streamEngineInstall: vi.fn((_k: string, _e: unknown, _err: unknown, _c?: unknown) => () => {}),
  enableFastDownloads: vi.fn(async () => {}),
  getRecommendedModel: vi.fn(async (): Promise<unknown> => null),
}));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  return {
    ...actual,
    getTransport: () => ({ ...transport, subscribe: vi.fn(() => () => {}), onEventStreamOpen: vi.fn(() => () => {}) }),
  };
});
vi.mock('../../../src/services/transport/api/setup', () => api);
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn(async () => true) }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { cleanupDownloadEvents, initDownloadEvents } from '../../../src/services/downloadsEvents';
import { takeQueueSnapshot } from '../../../src/services/downloadsRegistry';
import { cleanupModelEvents, initModelEvents, refreshModels } from '../../../src/services/modelEvents';
import { applyProxyStatus, resetProxyState } from '../../../src/services/proxyRegistry';
import { ADVANCE_MS } from '../../../src/places/setup/Step';
import { proofAnswered } from '../../../src/readings/connect';
import { formatBudget, formatRecommendation } from '../../../src/utils/format';
import { queueSnapshot, runningRow } from '../fixtures/downloads';
import { guiModel } from '../fixtures/model';
import { renderGated } from './harness';
import { DOWN, engine, REC, setupStatus, UP } from './setupFixtures';

const QWEN = guiModel({ id: 7, name: 'Qwen3-8B', openaiId: 'Qwen3-8B', hfRepoId: REC.repo, sizeBytes: 5_000_000_000 });
const PROOF = { answered_by: 'Qwen3-8B', asked_for: 'Qwen3-8B', elapsed_ms: 3200 };

/** The download Get it queued, a quarter of the way in. */
const ROW = runningRow({
  id: REC.repo,
  model_id: REC.repo,
  quantization: undefined,
  text: { title: 'Qwen3-8B', status: 'Downloading', bytes: '1.25 GiB / 5.00 GiB', percent: '25.0%', speed: '52.0 MB/s', eta: 'ETA 1m 12s' },
});

function start(status = setupStatus()) {
  api.getSetupStatus.mockImplementation(async () => status);
  initModelEvents();
  initDownloadEvents();
  return { ...renderGated('/library'), user: userEvent.setup() };
}

const step = (title: string) => screen.findByRole('region', { name: title });
const press = (user: ReturnType<typeof userEvent.setup>, name: string) => user.click(screen.getByRole('button', { name }));

/** From step 1 to `title`, pressing Continue on each step before it. */
async function reach(user: ReturnType<typeof userEvent.setup>, title: string) {
  await step('This Mac');
  for (const at of ['This Mac', 'Engines', 'First model', 'Endpoint']) {
    if (at === title) return;
    const region = await step(at);
    if (at === 'First model') await user.click(within(region).getByRole('button', { name: 'Skip' }));
    else await waitFor(() => expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled()).then(() => press(user, 'Continue'));
  }
}

describe('setup', () => {
  beforeEach(() => {
    for (const mock of [...Object.values(transport), ...Object.values(api)]) mock.mockClear();
    library.models = [];
    queue.snapshot = queueSnapshot();
    api.getEngineStatus.mockImplementation(async (kind: string) => engine(kind as 'llama', true));
    api.getRecommendedModel.mockImplementation(async () => REC);
    transport.startProxy.mockImplementation(async () => {
      applyProxyStatus(UP);
      return UP;
    });
    transport.proveProxy.mockImplementation(async () => PROOF);
    transport.getSettings.mockImplementation(async () => ({ proxyAutostart: false }));
    act(() => applyProxyStatus(DOWN));
  });
  afterEach(() => {
    act(() => resetProxyState());
    cleanupModelEvents();
    cleanupDownloadEvents();
  });

  it('is the window while setup is not done, and is not drawn once it is', async () => {
    start();
    expect(await step('This Mac')).toHaveTextContent('1 of 5');
    expect(screen.queryByRole('main', { name: 'Library' })).not.toBeInTheDocument();
  });

  it('opens the library, not setup, once setup is done or when the status cannot be read', async () => {
    start(setupStatus({ setupCompleted: true }));
    expect(await screen.findByRole('main', { name: 'Library' })).toBeInTheDocument();
    expect(screen.queryByRole('main', { name: 'Setup' })).not.toBeInTheDocument();
  });

  it('opens the library when the gate cannot read the status', async () => {
    api.getSetupStatus.mockRejectedValue(new Error('down'));
    initModelEvents();
    renderGated('/library');
    expect(await screen.findByRole('main', { name: 'Library' })).toBeInTheDocument();
  });

  it("says it can't reach gglib when setup's own read fails, and Try again reads again", async () => {
    const status = setupStatus();
    api.getSetupStatus.mockResolvedValueOnce(status).mockRejectedValueOnce(new Error('connection refused'));
    initModelEvents();
    renderGated('/library');
    const user = userEvent.setup();

    expect(await screen.findByRole('alert')).toHaveTextContent("Can't reach gglib.");
    api.getSetupStatus.mockResolvedValue(status);
    await press(user, 'Try again');
    expect(await step('This Mac')).toBeInTheDocument();
  });

  it("says This Mac in the daemon's words, and Change saves the folder typed and shows the one read back", async () => {
    transport.getModelsDirectory.mockResolvedValue({ path: '/Volumes/big/models', source: 'explicit', default_path: '/x', exists: true, writable: true });
    const { user } = start();
    const region = await step('This Mac');

    expect(region).toHaveTextContent(`macOS ARM64 (Metal) · ${formatBudget(REC)}`);
    const folder = within(region).getByRole('group', { name: 'Models folder' });
    expect(folder).toHaveTextContent('/Users/me/.local/share/llama_models(will be created)');

    await user.click(within(folder).getByRole('button', { name: 'Change' }));
    const field = screen.getByRole('textbox', { name: 'Models folder' });
    await user.clear(field);
    await user.type(field, ' /Volumes/big/models ');
    await press(user, 'Save');

    await waitFor(() => expect(transport.setModelsDirectory).toHaveBeenCalledWith('/Volumes/big/models'));
    const shown = await within(region).findByRole('group', { name: 'Models folder' });
    expect(shown).toHaveTextContent('/Volumes/big/models');
    expect(shown).not.toHaveTextContent('will be created');
  });

  it('leaves the memory out when nothing fits, and says so on First model', async () => {
    api.getRecommendedModel.mockResolvedValue(null);
    const { user } = start();
    const region = await step('This Mac');
    expect(region).toHaveTextContent('macOS ARM64 (Metal)');
    expect(region).not.toHaveTextContent('unified memory');

    await reach(user, 'First model');
    const first = await step('First model');
    expect(first).toHaveTextContent('Nothing gglib would suggest fits.');
    expect(within(first).queryByRole('button', { name: 'Get it' })).not.toBeInTheDocument();
  });

  it("offers the suggestion in the route's words, and Get it follows the download in its row's words", async () => {
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('First model');

    expect(region).toHaveTextContent(REC.repo);
    expect(region).toHaveTextContent(formatRecommendation(REC));
    await user.click(within(region).getByRole('button', { name: 'Get it' }));

    expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: REC.repo, quantization: undefined });
    act(() => takeQueueSnapshot(queueSnapshot({ revision: 5, active: ROW })));
    expect(await within(region).findByText('1.25 GiB / 5.00 GiB · 52.0 MB/s · ETA 1m 12s')).toBeInTheDocument();
    expect(within(region).getByRole('progressbar', { name: 'Qwen3-8B downloaded' })).toHaveAttribute('aria-valuenow', '25');
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled();
  });

  it("says a download that failed in the daemon's words, and Retry queues it again", async () => {
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('First model');
    await user.click(within(region).getByRole('button', { name: 'Get it' }));
    act(() =>
      takeQueueSnapshot(
        queueSnapshot({ revision: 6, finished: [{ id: REC.repo, title: 'Qwen3-8B', outcome: { kind: 'failed', error: 'disk full' }, text: 'Qwen3-8B failed: disk full' }] }),
      ),
    );

    expect(await within(region).findByRole('alert')).toHaveTextContent('Qwen3-8B failed: disk full');
    await user.click(within(region).getByRole('button', { name: 'Retry' }));
    expect(transport.queueDownload).toHaveBeenCalledTimes(2);
  });

  it('skips the model: Skip goes on, queueing nothing and proving nothing', async () => {
    const { user } = start();
    await reach(user, 'Endpoint');
    expect(await step('Endpoint')).toBeInTheDocument();
    expect(transport.queueDownload).not.toHaveBeenCalled();
    await waitFor(() => expect(transport.startProxy).toHaveBeenCalledWith({}));
    expect(transport.proveProxy).not.toHaveBeenCalled();
  });

  it("joins another machine's models with the pairing string, trimmed", async () => {
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('First model');

    await user.click(within(region).getByRole('button', { name: "Use another machine's models ›" }));
    await user.type(within(region).getByRole('textbox', { name: 'Paste a pairing string' }), '  abc-123 ');
    await user.click(within(region).getByRole('button', { name: 'Join' }));

    expect(transport.joinRemote).toHaveBeenCalledWith({ pairing: 'abc-123' });
    expect(await within(region).findByText('Joined. Its models are in your Library.')).toBeInTheDocument();
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled();
  });

  it('turns the endpoint on, waits on the download for the proof without holding anyone, then proves the new model', async () => {
    const { user } = start();
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    act(() => takeQueueSnapshot(queueSnapshot({ revision: 5, active: ROW })));
    await press(user, 'Continue');
    const region = await step('Endpoint');

    await waitFor(() => expect(transport.startProxy).toHaveBeenCalledWith({}));
    expect(await within(region).findByText(UP.base_url!)).toBeInTheDocument();
    expect(region).toHaveTextContent('On');
    expect(region).toHaveTextContent('The test runs when Qwen3-8B finishes.');
    expect(transport.proveProxy).not.toHaveBeenCalled();
    // Nothing waits for it: Continue is there while the download runs.
    await press(user, 'Continue');
    expect(await step('Connect')).toBeInTheDocument();

    library.models = [QWEN];
    await act(() => refreshModels());
    await waitFor(() => expect(transport.proveProxy).toHaveBeenCalledWith(7));
    await press(user, 'Back');
    expect(await within(await step('Endpoint')).findByText(proofAnswered(PROOF))).toBeInTheDocument();
    expect(transport.proveProxy).toHaveBeenCalledTimes(1);
  });

  it("says a proof that failed in the daemon's words, with Retry", async () => {
    library.models = [QWEN];
    transport.proveProxy.mockRejectedValueOnce(new Error('the model did not load'));
    const { user } = start();
    await reach(user, 'First model');
    // The library had a model: First model is already done, and moves on.
    const region = await step('Endpoint');

    expect(await within(region).findByRole('alert')).toHaveTextContent('the model did not load');
    await user.click(within(region).getByRole('button', { name: 'Retry' }));
    expect(await within(region).findByText(proofAnswered(PROOF))).toBeInTheDocument();
  });

  it('saves Start the endpoint when gglib launches by its wire name', async () => {
    const { user } = start();
    await reach(user, 'Endpoint');
    const region = await step('Endpoint');
    const box = await within(region).findByRole('checkbox', { name: 'Start the endpoint when gglib launches' });
    await waitFor(() => expect(box).toBeEnabled());

    await user.click(box);
    expect(transport.updateSettings).toHaveBeenCalledWith({ proxyAutostart: true });
  });

  it('fills the Model ID in, and Open gglib writes setup done and lands on the new model', async () => {
    const { user, at } = start();
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    library.models = [QWEN];
    await act(() => refreshModels());
    await press(user, 'Continue');
    await press(user, 'Continue');
    const region = await step('Connect');

    const block = await within(region).findByRole('region', { name: 'Connect a client' });
    expect(await within(block).findByText('Qwen3-8B', { selector: 'code' })).toBeInTheDocument();
    await press(user, 'Open gglib');

    await waitFor(() => expect(transport.updateSettings).toHaveBeenCalledWith({ setupCompleted: true }));
    expect(await screen.findByRole('main', { name: 'Library' })).toBeInTheDocument();
    expect(at()).toBe('/models/7');
    expect(screen.queryByRole('main', { name: 'Setup' })).not.toBeInTheDocument();
  });

  it("lands on the Library's Hugging Face search after Choose another", async () => {
    const { user, at } = start();
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Choose another' }));
    await press(user, 'Continue');
    await press(user, 'Open gglib');

    expect(await screen.findByRole('main', { name: 'Library' })).toBeInTheDocument();
    expect(at()).toBe('/library?add=browse');
  });

  it('reads Already done after gglib up, and moves on by itself without a download, a start or a proof', async () => {
    library.models = [QWEN];
    act(() => applyProxyStatus(UP));
    const { user } = start(setupStatus({ llamaInstalled: true }));
    await reach(user, 'First model');

    const first = await step('First model');
    expect(first).toHaveTextContent('Already done');
    const endpoint = await screen.findByRole('region', { name: 'Endpoint' }, { timeout: ADVANCE_MS * 3 });
    expect(endpoint).toHaveTextContent('Already done');
    expect(await screen.findByRole('region', { name: 'Connect' }, { timeout: ADVANCE_MS * 3 })).toBeInTheDocument();
    expect(transport.queueDownload).not.toHaveBeenCalled();
    expect(transport.startProxy).not.toHaveBeenCalled();
    expect(transport.proveProxy).not.toHaveBeenCalled();

    // Back to a step that moved on by itself stays there.
    await press(user, 'Back');
    expect(await step('Endpoint')).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, ADVANCE_MS + 200));
    expect(screen.getByRole('region', { name: 'Endpoint' })).toBeInTheDocument();
  });
});
