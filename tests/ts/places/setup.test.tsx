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
    api.getRecommendedModel.mockRejectedValue(new Error('connection refused'));
    const { user } = start();

    expect(await screen.findByRole('alert')).toHaveTextContent("Can't reach gglib.");
    api.getRecommendedModel.mockResolvedValue(REC);
    await press(user, 'Try again');
    expect(await step('This Mac')).toBeInTheDocument();
  });

  it('reads the setup status once, at the gate, and setup uses that read', async () => {
    const { user } = start();
    await reach(user, 'First model');
    // StrictMode runs the gate's effect twice; the place adds no read of its own.
    expect(api.getSetupStatus.mock.calls.length).toBeLessThanOrEqual(2);
    const before = api.getSetupStatus.mock.calls.length;
    await press(user, 'Back');
    await press(user, 'Back');
    expect(api.getSetupStatus).toHaveBeenCalledTimes(before);
  });

  it("says a models folder that cannot be written in the served word", async () => {
    const status = setupStatus();
    start(setupStatus({ modelsDirectory: { ...status.modelsDirectory, exists: false, writable: false, state: 'not_writable' } }));
    const folder = within(await step('This Mac')).getByRole('group', { name: 'Models folder' });
    expect(folder).toHaveTextContent('/Users/me/.local/share/llama_models(Not writable)');
    expect(folder).not.toHaveTextContent('Will be created');
  });

  it("says This Mac in the daemon's words, and Change saves the folder typed and shows the one read back", async () => {
    // The route names a stored folder `environment`, as it names GGLIB_MODELS_DIR (`paths/models.rs`).
    transport.getModelsDirectory.mockResolvedValue({ path: '/Volumes/big/models', source: 'environment', default_path: '/x', exists: true, writable: true, state: 'writable' });
    const { user } = start();
    const region = await step('This Mac');

    expect(region).toHaveTextContent('macOS ARM64 (Metal) · 43.2 GiB unified memory');
    const folder = within(region).getByRole('group', { name: 'Models folder' });
    expect(folder).toHaveTextContent('/Users/me/.local/share/llama_models(Will be created)');

    await user.click(within(folder).getByRole('button', { name: 'Change' }));
    const field = screen.getByRole('textbox', { name: 'Models folder' });
    // No way on while a folder is being typed.
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeDisabled();
    await user.clear(field);
    await user.type(field, ' /Volumes/big/models ');
    await press(user, 'Save');

    await waitFor(() => expect(transport.setModelsDirectory).toHaveBeenCalledWith('/Volumes/big/models'));
    const shown = await within(region).findByRole('group', { name: 'Models folder' });
    expect(shown).toHaveTextContent('/Volumes/big/models');
    expect(shown).not.toHaveTextContent('Will be created');
    // Where it was set is not said: the route cannot tell a chosen folder from the variable.
    expect(region).not.toHaveTextContent('GGLIB_MODELS_DIR');
  });

  it("keeps the field and says a folder the daemon refused in its words", async () => {
    transport.setModelsDirectory.mockRejectedValueOnce(new Error('the folder is not writable'));
    const { user } = start();
    const region = await step('This Mac');
    await user.click(within(region).getByRole('button', { name: 'Change' }));
    await user.type(screen.getByRole('textbox', { name: 'Models folder' }), 'x');
    await press(user, 'Save');

    expect(await within(region).findByRole('alert')).toHaveTextContent('the folder is not writable');
    expect(screen.getByRole('textbox', { name: 'Models folder' })).toBeInTheDocument();
  });

  it('names no memory figure of its own when nothing fits, and says so on First model', async () => {
    api.getRecommendedModel.mockResolvedValue(null);
    const { user } = start();
    const region = await step('This Mac');
    await waitFor(() => expect(region).toHaveTextContent('macOS ARM64 (Metal)'));
    expect(region).not.toHaveTextContent('GiB');

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
    expect(region).toHaveTextContent('Strong tool calling at 8B. Needs 6.5 GiB of 43.2 GiB unified memory.');
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

  it("says a refusal as it came, with Retry", async () => {
    transport.queueDownload.mockRejectedValueOnce(new Error('queue full: max 10 downloads'));
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('First model');
    await user.click(within(region).getByRole('button', { name: 'Get it' }));

    expect(await within(region).findByRole('alert')).toHaveTextContent('queue full: max 10 downloads');
    await user.click(within(region).getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledTimes(2));
  });

  it('sets faster downloads up on Get it, before the download, and not before', async () => {
    const order: string[] = [];
    api.enableFastDownloads.mockImplementation(async () => void order.push('fast'));
    transport.queueDownload.mockImplementation(async () => {
      order.push('queue');
      return { id: REC.repo };
    });
    const { user } = start(setupStatus({ fastDownloads: 'off', pythonAvailable: true }));
    await reach(user, 'First model');
    expect(api.enableFastDownloads).not.toHaveBeenCalled();

    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    await waitFor(() => expect(order).toEqual(['fast', 'queue']));
    await press(user, 'Back');
    expect(within(await step('Engines')).getByRole('group', { name: 'Faster downloads' })).toHaveTextContent(/^Faster downloadsOn$/);
  });

  it('queues the model only once the set-up has landed, saying so meanwhile; a Skip then queues nothing', async () => {
    let landed: () => void = () => {};
    api.enableFastDownloads.mockImplementationOnce(() => new Promise<void>((resolve) => (landed = resolve)));
    const { user } = start(setupStatus({ fastDownloads: 'off', pythonAvailable: true }));
    await reach(user, 'First model');
    const first = await step('First model');
    await user.click(within(first).getByRole('button', { name: 'Get it' }));
    expect(await within(first).findByText('setting up faster downloads first')).toBeInTheDocument();
    await act(async () => {});
    expect(transport.queueDownload).not.toHaveBeenCalled();

    // Skip while pip runs: the choice sticks, and the set-up landing queues nothing.
    await user.click(within(first).getByRole('button', { name: 'Skip' }));
    expect(await step('Endpoint')).toBeInTheDocument();
    await act(async () => landed());
    await act(async () => {});
    expect(transport.queueDownload).not.toHaveBeenCalled();
    expect(await step('Endpoint')).not.toHaveTextContent('The test runs when');
  });

  it('queues the model once the set-up lands', async () => {
    let landed: () => void = () => {};
    api.enableFastDownloads.mockImplementationOnce(() => new Promise<void>((resolve) => (landed = resolve)));
    const { user } = start(setupStatus({ fastDownloads: 'off', pythonAvailable: true }));
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    await act(async () => {});
    expect(transport.queueDownload).not.toHaveBeenCalled();
    await act(async () => landed());
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledTimes(1));
  });

  it("warns of a failed accelerator in the daemon's words and downloads anyway, and Retry sets it up", async () => {
    api.enableFastDownloads.mockRejectedValueOnce(new Error('pip failed'));
    const { user } = start(setupStatus({ fastDownloads: 'off', pythonAvailable: true }));
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledTimes(1));

    await press(user, 'Back');
    const fast = within(await step('Engines')).getByRole('group', { name: 'Faster downloads' });
    expect(within(fast).getByRole('status')).toHaveTextContent('pip failed');
    expect(fast).toHaveTextContent('downloads still work');
    await user.click(within(fast).getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(fast).toHaveTextContent(/^Faster downloadsOn$/));
    expect(api.enableFastDownloads).toHaveBeenCalledTimes(2);
  });

  it('sets nothing up on Get it where Python 3 is not there', async () => {
    const { user } = start(setupStatus({ fastDownloads: 'off', pythonAvailable: false }));
    await reach(user, 'First model');
    await user.click(within(await step('First model')).getByRole('button', { name: 'Get it' }));
    await waitFor(() => expect(transport.queueDownload).toHaveBeenCalledTimes(1));
    expect(api.enableFastDownloads).not.toHaveBeenCalled();
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

  it("says a pairing the daemon refused in its words, and joins nothing", async () => {
    transport.joinRemote.mockRejectedValueOnce(new Error('the pairing code has expired'));
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('First model');
    await user.click(within(region).getByRole('button', { name: "Use another machine's models ›" }));
    await user.type(within(region).getByRole('textbox', { name: 'Paste a pairing string' }), 'abc-123');
    await user.click(within(region).getByRole('button', { name: 'Join' }));

    expect(await within(region).findByRole('alert')).toHaveTextContent('the pairing code has expired');
    expect(within(region).queryByText('Joined. Its models are in your Library.')).not.toBeInTheDocument();
    expect(within(region).queryByRole('button', { name: 'Continue' })).not.toBeInTheDocument();
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

  it('proves a pinned endpoint with its pin, naming no model, and so does Retry', async () => {
    library.models = [QWEN];
    const pinned = { ...UP, state: 'pinned' as const, pinned_model: 'Qwen3-8B', pinned_to: 'Pinned to Qwen3-8B', pinned_model_id: 7 };
    transport.startProxy.mockImplementation(async () => {
      applyProxyStatus(pinned);
      return pinned;
    });
    transport.proveProxy.mockRejectedValueOnce(new Error('the model did not load'));
    const { user } = start();
    await reach(user, 'First model');
    const region = await step('Endpoint');

    expect(await within(region).findByRole('alert')).toHaveTextContent('the model did not load');
    expect(transport.proveProxy).toHaveBeenLastCalledWith(null);
    await user.click(within(region).getByRole('button', { name: 'Retry' }));
    expect(await within(region).findByText(proofAnswered(PROOF))).toBeInTheDocument();
    expect(transport.proveProxy).toHaveBeenCalledTimes(2);
    expect(transport.proveProxy).toHaveBeenLastCalledWith(null);
  });

  it('Copy puts the address on the clipboard and says Copied beside it', async () => {
    const { user } = start();
    await reach(user, 'Endpoint');
    const region = await step('Endpoint');
    await within(region).findByText(UP.base_url!);
    await user.click(within(region).getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toBe(UP.base_url);
    expect(await within(region).findByRole('status')).toHaveTextContent('Copied');
    expect(screen.getByTestId('toasts')).toBeEmptyDOMElement();
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

  it('lets Open gglib in when setup done cannot be written', async () => {
    transport.updateSettings.mockRejectedValueOnce(new Error('settings are read-only'));
    const { user, at } = start();
    await reach(user, 'Endpoint');
    await press(user, 'Continue');
    await press(user, 'Open gglib');

    expect(await screen.findByRole('main', { name: 'Library' })).toBeInTheDocument();
    expect(at()).toBe('/library');
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

  it('moves an Already done step on once, ever: Continue before it does, then Back, and it stays', async () => {
    library.models = [QWEN];
    act(() => applyProxyStatus(UP));
    const { user } = start(setupStatus({ llamaInstalled: true }));
    await reach(user, 'First model');
    expect(await step('First model')).toHaveTextContent('Already done');

    await press(user, 'Continue');
    expect(await step('Endpoint')).toBeInTheDocument();
    await press(user, 'Back');
    expect(await step('First model')).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, ADVANCE_MS + 300));
    expect(screen.getByRole('region', { name: 'First model' })).toBeInTheDocument();
  });

  it('moves an Already done step on once, ever: left by Back before it does, it stays when come back to', async () => {
    library.models = [QWEN];
    act(() => applyProxyStatus(UP));
    const { user } = start(setupStatus({ llamaInstalled: true }));
    await reach(user, 'First model');
    expect(await step('First model')).toHaveTextContent('Already done');

    await press(user, 'Back');
    expect(await step('Engines')).toBeInTheDocument();
    await press(user, 'Continue');
    expect(await step('First model')).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, ADVANCE_MS + 300));
    expect(screen.getByRole('region', { name: 'First model' })).toBeInTheDocument();
  });
});
