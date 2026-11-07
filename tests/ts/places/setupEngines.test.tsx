/**
 * Setup's Engines step, behind the gate: llama.cpp installing on arrival
 * through the daemon's stream (SU-1) with its progress, then ready, then
 * stable-diffusion.cpp; llama.cpp's failure an error with Retry and
 * stable-diffusion.cpp's a warning (SU-2); a stream that closes without a
 * result ending with a reason; the source build's sentence, Check again,
 * and the poll that finds the build; faster downloads set up where Python
 * 3 is there and not otherwise; and an unreadable engine with Retry.
 *
 * The install stream is played by the test through the callbacks
 * `streamEngineInstall` was handed.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const transport = vi.hoisted(() => ({
  listModels: vi.fn(async () => [] as unknown[]),
  getDownloadQueue: vi.fn(async () => null),
  getSettings: vi.fn(async () => ({})),
  updateSettings: vi.fn(async (u: Record<string, unknown>) => u),
}));
type Handlers = { onEvent: (e: unknown) => void; onError: (m: string) => void; onClose?: () => void };
const api = vi.hoisted(() => ({
  streams: [] as { kind: string; on: Handlers }[],
  getSetupStatus: vi.fn(async (): Promise<unknown> => ({})),
  getEngineStatus: vi.fn(async (_k: string): Promise<unknown> => ({})),
  streamEngineInstall: vi.fn(),
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
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

import { BUILD_POLL_MS, CLOSED_WITHOUT_RESULT } from '../../../src/places/setup/useEngines';
import { INSTALL_PHASE_LABELS, type EngineStatus, type RuntimeKind } from '../../../src/types/setup';
import { renderGated } from './harness';
import { engine, setupStatus } from './setupFixtures';

/** What each engine's status read answers, the last one kept. */
let reads: Record<RuntimeKind, EngineStatus[]>;

function answer(kind: RuntimeKind, ...statuses: EngineStatus[]) {
  reads[kind] = statuses;
}

async function reachEngines(status = setupStatus()) {
  api.getSetupStatus.mockImplementation(async () => status);
  renderGated('/library');
  const user = userEvent.setup();
  await screen.findByRole('region', { name: 'This Mac' });
  await user.click(screen.getByRole('button', { name: 'Continue' }));
  return { user, region: await screen.findByRole('region', { name: 'Engines' }) };
}

/** The install stream for `kind`, once the step has opened it. */
async function stream(kind: RuntimeKind, nth = 1): Promise<Handlers> {
  await waitFor(() => expect(api.streams.filter((s) => s.kind === kind)).toHaveLength(nth));
  return api.streams.filter((s) => s.kind === kind)[nth - 1].on;
}

const line = (region: HTMLElement, name: string) => within(region).getByRole('group', { name });

describe("setup's engines", () => {
  beforeEach(() => {
    for (const mock of [...Object.values(transport), api.getSetupStatus, api.getEngineStatus, api.enableFastDownloads]) mock.mockClear();
    api.streams = [];
    reads = { llama: [engine('llama')], stable_diffusion: [engine('stable_diffusion')] };
    api.getEngineStatus.mockImplementation(async (kind: string) => {
      const list = reads[kind as RuntimeKind];
      return list.length > 1 ? list.shift() : list[0];
    });
    api.streamEngineInstall.mockImplementation((kind: string, onEvent: Handlers['onEvent'], onError: Handlers['onError'], onClose?: () => void) => {
      api.streams.push({ kind, on: { onEvent, onError, onClose } });
      return () => {};
    });
    api.enableFastDownloads.mockImplementation(async () => {});
  });
  afterEach(() => vi.useRealTimers());

  it("installs llama.cpp on arrival through the daemon's stream, with its progress, then ready, then stable-diffusion.cpp", async () => {
    const { region } = await reachEngines();
    const llama = await stream('llama');
    expect(api.streams.map((s) => s.kind)).toEqual(['llama']);

    act(() => llama.onEvent({ type: 'phase_started', phase: 'fetch_release' }));
    expect(await within(region).findByText(INSTALL_PHASE_LABELS.fetch_release)).toBeInTheDocument();
    act(() => llama.onEvent({ type: 'progress', downloaded: 5_000_000, total: 20_000_000, rate_bps: 2_500_000, eta_seconds: 6 }));
    expect(await within(region).findByText('25.0%')).toBeInTheDocument();
    expect(line(region, 'llama.cpp')).toHaveTextContent('installing');
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeDisabled();

    answer('llama', engine('llama', true));
    act(() => llama.onEvent({ type: 'completed', version: 'b11429' }));
    await waitFor(() => expect(line(region, 'llama.cpp')).toHaveTextContent('ready'));
    await stream('stable_diffusion');
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled();
    expect(within(region).queryByRole('button', { name: 'Skip' })).not.toBeInTheDocument();
  });

  it("says llama.cpp's failure in the daemon's words as an error, and Retry installs it again", async () => {
    const { user, region } = await reachEngines();
    const llama = await stream('llama');
    act(() => llama.onEvent({ type: 'failed', message: 'Failed to install llama.cpp: no asset for this platform' }));

    const alert = await within(line(region, 'llama.cpp')).findByRole('alert');
    expect(alert).toHaveTextContent('Failed to install llama.cpp: no asset for this platform');
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeDisabled();
    expect(within(region).getByRole('button', { name: 'Skip' })).toBeInTheDocument();

    await user.click(within(alert).getByRole('button', { name: 'Retry' }));
    await stream('llama', 2);
  });

  it('ends an install whose stream closes without a result, with the reason', async () => {
    const { region } = await reachEngines();
    const llama = await stream('llama');
    act(() => llama.onClose?.());

    expect(await within(line(region, 'llama.cpp')).findByRole('alert')).toHaveTextContent(CLOSED_WITHOUT_RESULT);
  });

  it("ends an install the daemon refuses outright, in the daemon's words", async () => {
    const { region } = await reachEngines();
    const llama = await stream('llama');
    act(() => llama.onError('a model is running on llama-server: stop Qwen3-8B first'));

    expect(await within(line(region, 'llama.cpp')).findByRole('alert')).toHaveTextContent('stop Qwen3-8B first');
  });

  it("says stable-diffusion.cpp's failure as a warning, skipped, with chat going on", async () => {
    answer('llama', engine('llama', true));
    const { region } = await reachEngines();
    const sd = await stream('stable_diffusion');
    act(() => sd.onEvent({ type: 'failed', message: 'Failed to install stable-diffusion.cpp: no asset' }));

    const drawing = line(region, 'stable-diffusion.cpp (for drawing)');
    await waitFor(() => expect(drawing).toHaveTextContent('skipped — chat works without it'));
    expect(within(drawing).getByRole('status')).toHaveTextContent('Failed to install stable-diffusion.cpp: no asset');
    expect(within(region).queryByRole('alert')).not.toBeInTheDocument();
    expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled();
  });

  it('names the source build and its command, installs nothing for it, and finds the build by Check again and by itself', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    const source = engine('llama', false, { download: null, downloadUnavailable: 'No asset.' }, { updateDownloads: false });
    answer('llama', source);
    answer('stable_diffusion', engine('stable_diffusion', true));
    const { user, region } = await reachEngines();

    const llama = line(region, 'llama.cpp');
    expect(await within(llama).findByText(/This Mac needs llama.cpp built from source\. In a terminal:/)).toBeInTheDocument();
    expect(within(llama).getByText('gglib config llama install', { selector: 'code' })).toBeInTheDocument();
    expect(api.streams).toHaveLength(0);

    const before = api.getEngineStatus.mock.calls.length;
    await user.click(within(llama).getByRole('button', { name: 'Check again' }));
    await waitFor(() => expect(api.getEngineStatus.mock.calls.length).toBeGreaterThan(before));

    answer('llama', engine('llama', true));
    act(() => vi.advanceTimersByTime(BUILD_POLL_MS));
    await waitFor(() => expect(llama).toHaveTextContent('ready'));
    expect(api.streams).toHaveLength(0);
  });

  it('sets faster downloads up on arrival where Python 3 is there, and says so', async () => {
    const { region } = await reachEngines(setupStatus({ fastDownloadReady: false, pythonAvailable: true }));
    await waitFor(() => expect(api.enableFastDownloads).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(line(region, 'Faster downloads')).toHaveTextContent('set up'));
  });

  it('says faster downloads need Python 3 where it is not there, and sets nothing up', async () => {
    const { region } = await reachEngines(setupStatus({ fastDownloadReady: false, pythonAvailable: false }));
    expect(line(region, 'Faster downloads')).toHaveTextContent('needs Python 3; downloads still work');
    await stream('llama');
    expect(api.enableFastDownloads).not.toHaveBeenCalled();
  });

  it("warns of a failed accelerator in the daemon's words, and Retry sets it up again", async () => {
    api.enableFastDownloads.mockRejectedValueOnce(new Error('pip failed'));
    const { user, region } = await reachEngines(setupStatus({ fastDownloadReady: false, pythonAvailable: true }));
    const fast = line(region, 'Faster downloads');

    expect(await within(fast).findByRole('status')).toHaveTextContent('pip failed');
    expect(fast).toHaveTextContent('downloads still work');
    await user.click(within(fast).getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(fast).toHaveTextContent('set up'));
    expect(api.enableFastDownloads).toHaveBeenCalledTimes(2);
  });

  it('says an engine it cannot read, and Retry reads and installs', async () => {
    api.getEngineStatus.mockRejectedValueOnce(new Error('no status'));
    const { user, region } = await reachEngines();
    const alert = await within(line(region, 'llama.cpp')).findByRole('alert');
    expect(alert).toHaveTextContent('no status');

    await user.click(within(alert).getByRole('button', { name: 'Retry' }));
    await stream('llama');
  });

  it('installs only what is missing, and leaves an older llama.cpp to Settings and the banner', async () => {
    answer('llama', engine('llama', true, {}, { standing: 'older', installedRelease: 'b10327', updateDownloads: true }));
    const { region } = await reachEngines();

    await stream('stable_diffusion');
    expect(api.streams.map((s) => s.kind)).toEqual(['stable_diffusion']);
    expect(line(region, 'llama.cpp')).toHaveTextContent('ready');
  });

  it('goes on without llama.cpp by Skip', async () => {
    const { user, region } = await reachEngines();
    await stream('llama');
    await user.click(within(region).getByRole('button', { name: 'Skip' }));
    expect(await screen.findByRole('region', { name: 'First model' })).toBeInTheDocument();
  });

  it('starts nothing twice after Back and Continue', async () => {
    answer('llama', engine('llama', true));
    answer('stable_diffusion', engine('stable_diffusion', true));
    const { user, region } = await reachEngines(setupStatus({ fastDownloadReady: false, pythonAvailable: true }));
    await waitFor(() => expect(within(region).getByRole('button', { name: 'Continue' })).toBeEnabled());
    await user.click(within(region).getByRole('button', { name: 'Back' }));
    await user.click(await screen.findByRole('button', { name: 'Continue' }));
    await screen.findByRole('region', { name: 'Engines' });

    expect(api.enableFastDownloads).toHaveBeenCalledTimes(1);
    expect(api.getEngineStatus).toHaveBeenCalledTimes(2);
    expect(api.streams).toHaveLength(0);
  });
});
