/**
 * The setup wizard's engines step: llama.cpp, then stable-diffusion.cpp,
 * each installed from its pinned download by one button.
 *
 * The step reads each engine's status from the daemon and installs over the
 * stream the Settings engine card reads, drawn by the same `InstallProgress`,
 * so this pins the wizard's side: the order of the installs and the requests
 * they make, the progress, a required engine's failure against an optional
 * one's, and what is left to the terminal. The transport is the real one
 * over a stubbed `fetch`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { SetupWizard } from '../../../src/components/SetupWizard';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import {
  INSTALL_PHASE_LABELS,
  type EngineStatus,
  type LlamaProgressEvent,
  type PinState,
  type RuntimeKind,
  type SetupStatus,
} from '../../../src/types/setup';
import { formatBytes, formatDuration, formatRate } from '../../../src/utils/format';

const encoder = new TextEncoder();

const STATUS: SetupStatus = {
  setupCompleted: false,
  llamaInstalled: false,
  llamaPin: { standing: 'missing', installedRelease: null, targetRelease: 'b11429', built: null, rebuilds: false, updateDownloads: true },
  llamaCanDownload: true,
  llamaPlatformDescription: 'macOS ARM64 (Metal)',
  gpuInfo: {
    hasMetal: true,
    hasNvidia: false,
    hasVulkan: false,
    cudaVersion: null,
    vulkanHeadersInstalled: false,
    vulkanGlslcInstalled: false,
    vulkanSpirvHeadersInstalled: false,
  },
  modelsDirectory: { path: '/models', source: 'default', default_path: '/models', exists: true, writable: true },
  pythonAvailable: true,
  fastDownloadReady: false,
  systemMemory: null,
};

const RELEASE: Record<RuntimeKind, string> = { llama: 'b11429', stable_diffusion: 'master-948-228c707' };
const WORD: Record<RuntimeKind, string> = { llama: 'llama', stable_diffusion: 'sd' };

/** `kind`'s status: missing with the Metal download, unless `installed`. */
function engine(
  kind: RuntimeKind,
  installed = false,
  overrides: Partial<EngineStatus> = {},
  pin: Partial<PinState> = {},
): EngineStatus {
  return {
    kind,
    installed,
    broken: null,
    binaryPath: `/data/${kind}`,
    install: installed ? { built: false, release: RELEASE[kind], platform: 'Metal', date: '2026-10-26T00:00:00Z' } : null,
    recordError: null,
    reports: null,
    pin: {
      standing: installed ? 'atPin' : 'missing',
      installedRelease: installed ? RELEASE[kind] : null,
      targetRelease: RELEASE[kind],
      built: installed ? false : null,
      rebuilds: false,
      updateDownloads: !installed,
      ...pin,
    },
    download: 'macOS ARM64 (Metal)',
    downloadUnavailable: null,
    warning: null,
    serving: null,
    installCommand: `gglib config ${WORD[kind]} install`,
    updateCommand: `gglib config ${WORD[kind]} update`,
    ...overrides,
  };
}

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

describe("the setup wizard's engines step", () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let statuses: Record<RuntimeKind, EngineStatus[]>;
  /** An engine whose install request the daemon answers with a 500. */
  let refused: RuntimeKind | null;
  /** Whether llama.cpp's status read is answered with a 500. */
  let unreadable: boolean;
  /** Whether the next read of llama.cpp's status waits for `releaseRead`. */
  let holdRead: boolean;
  let releaseRead: (() => void) | null;
  let bodies: Partial<Record<RuntimeKind, ReadableStreamDefaultController<Uint8Array>>>;

  /** One frame of `kind`'s install stream, as `install_slot::sse_frame` writes it. */
  const send = (kind: RuntimeKind, event: LlamaProgressEvent) =>
    bodies[kind]!.enqueue(encoder.encode(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`));

  beforeEach(() => {
    statuses = { llama: [], stable_diffusion: [] };
    refused = null;
    unreadable = false;
    holdRead = false;
    releaseRead = null;
    bodies = {};
    fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith('/setup-status')) return json(STATUS);
      for (const kind of ['llama', 'stable_diffusion'] as const) {
        if (url.endsWith(`/engines/${kind}`)) {
          if (kind === 'llama' && unreadable) {
            return new Response(JSON.stringify({ error: 'no status' }), {
              status: 500,
              headers: { 'content-type': 'application/json' },
            });
          }
          if (kind === 'llama' && holdRead) {
            holdRead = false;
            await new Promise<void>((resolve) => void (releaseRead = resolve));
          }
          return json(statuses[kind].length > 1 ? statuses[kind].shift() : (statuses[kind][0] ?? engine(kind)));
        }
        if (url.endsWith(`/engines/${kind}/install`) && refused === kind) {
          return new Response(JSON.stringify({ error: 'the daemon would not' }), {
            status: 500,
            headers: { 'content-type': 'application/json' },
          });
        }
        if (url.endsWith(`/engines/${kind}/install`)) {
          return new Response(
            new ReadableStream<Uint8Array>({ start: (controller) => void (bodies[kind] = controller) }),
            { status: 200, headers: { 'content-type': 'text/event-stream' } },
          );
        }
      }
      throw new Error(`unexpected request ${url}`);
    });
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => vi.unstubAllGlobals());

  const urls = () => (fetchMock.mock.calls as [string, RequestInit | undefined][]).map(([url]) => url);
  const installs = () =>
    (fetchMock.mock.calls as [string, RequestInit | undefined][]).filter(([url]) => url.endsWith('/install'));

  /** The summary's row for `label`, once the wizard is on its last step. */
  const summaryRow = (label: string) => screen.getByText(label).parentElement!;

  /** Walk to the engines step. */
  async function reachEngines() {
    render(<SetupWizard onComplete={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Get Started' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(await screen.findByRole('heading', { name: 'Engines' })).toBeInTheDocument();
  }

  it('offers no way on until the engines have been read', async () => {
    holdRead = true;
    await reachEngines();

    expect(screen.getByText('Reading the engines…')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Continue' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Back' })).toBeInTheDocument();
    await vi.waitFor(() => expect(releaseRead).toBeTypeOf('function'));
    releaseRead!();

    expect(await screen.findByRole('button', { name: 'Install' })).toBeInTheDocument();
  });

  async function pressInstall() {
    await reachEngines();
    await userEvent.click(await screen.findByRole('button', { name: 'Install' }));
    await vi.waitFor(() => expect(installs()).toHaveLength(1));
  }

  it("installs llama.cpp first, drawing the phase and the bytes as the daemon measured them", async () => {
    await pressInstall();

    const [url, init] = installs()[0];
    expect(url).toBe('/api/config/system/engines/llama/install');
    expect(init?.method).toBe('POST');

    send('llama', { type: 'phase_started', phase: 'fetch_release' });
    expect(await screen.findByText(INSTALL_PHASE_LABELS.fetch_release)).toBeInTheDocument();

    send('llama', { type: 'progress', downloaded: 5_000_000, total: 20_000_000, rate_bps: 2_500_000, eta_seconds: 6 });
    expect(await screen.findByText('25.0%')).toBeInTheDocument();
    expect(screen.getByText(`${formatBytes(5_000_000)} / ${formatBytes(20_000_000)}`)).toBeInTheDocument();
    expect(screen.getByText(formatRate(2_500_000))).toBeInTheDocument();
    expect(screen.getByText(`${formatDuration(6)} remaining`)).toBeInTheDocument();
    // No way forward or back while the install runs.
    expect(screen.queryByRole('button', { name: 'Skip' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });

  it('installs stable-diffusion.cpp once llama.cpp is in, then moves on with both', async () => {
    await pressInstall();
    statuses = { llama: [engine('llama', true)], stable_diffusion: [engine('stable_diffusion', true)] };

    send('llama', { type: 'completed', version: 'b11429' });
    await vi.waitFor(() => expect(installs()).toHaveLength(2));
    expect(installs()[1][0]).toBe('/api/config/system/engines/stable_diffusion/install');
    send('stable_diffusion', { type: 'phase_started', phase: 'download' });
    expect(await screen.findByText('Downloading stable-diffusion.cpp binaries…')).toBeInTheDocument();
    send('stable_diffusion', { type: 'completed', version: 'master-948-228c707' });

    expect(await screen.findByText('Installed: b11429')).toBeInTheDocument();
    expect(screen.getByText('Installed: master-948-228c707')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    expect(await screen.findByRole('heading', { name: 'Fast Downloads' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Skip' }));
    expect(await screen.findByText(/all set/)).toBeInTheDocument();
    expect(summaryRow('stable-diffusion.cpp')).toHaveTextContent('Installed');
    expect(summaryRow('stable-diffusion.cpp')).not.toHaveTextContent('Not installed');
  });

  it('says on the summary that stable-diffusion.cpp was not installed when it was skipped', async () => {
    await reachEngines();
    await userEvent.click(await screen.findByRole('button', { name: 'Skip' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Skip' }));

    expect(await screen.findByText(/all set/)).toBeInTheDocument();
    expect(summaryRow('stable-diffusion.cpp')).toHaveTextContent('Not installed');
  });

  it('says a failed read, still lets the wizard go on, and leaves the unread engine off the summary', async () => {
    unreadable = true;
    await reachEngines();

    expect(await screen.findByRole('alert')).toHaveTextContent('Could not read the engines');
    expect(screen.getByRole('button', { name: 'Back' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Skip' }));

    expect(await screen.findByText(/all set/)).toBeInTheDocument();
    expect(screen.queryByText('stable-diffusion.cpp')).not.toBeInTheDocument();
  });

  it("says llama.cpp's failure in the daemon's words, still installs stable-diffusion.cpp, and offers a retry", async () => {
    await pressInstall();
    statuses.stable_diffusion = [engine('stable_diffusion', true)];

    send('llama', { type: 'failed', message: 'Failed to install llama.cpp: no asset for this platform' });
    await vi.waitFor(() => expect(installs()).toHaveLength(2));
    send('stable_diffusion', { type: 'completed', version: 'master-948-228c707' });

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('llama.cpp did not install');
    expect(alert).toHaveTextContent('Failed to install llama.cpp: no asset for this platform');
    expect(alert).not.toHaveTextContent('Chat works without it');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Skip' })).toBeInTheDocument();
  });

  it("warns of stable-diffusion.cpp's failure without blocking chat", async () => {
    await pressInstall();
    statuses.llama = [engine('llama', true)];

    send('llama', { type: 'completed', version: 'b11429' });
    await vi.waitFor(() => expect(installs()).toHaveLength(2));
    send('stable_diffusion', { type: 'failed', message: 'Failed to install stable-diffusion.cpp: no asset' });

    expect(await screen.findByText('stable-diffusion.cpp did not install')).toBeInTheDocument();
    expect(screen.getByText(/Chat works without it/)).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(await screen.findByText('Installed: b11429')).toBeInTheDocument();
  });

  it('installs only what is missing, and leaves an older engine to Settings', async () => {
    statuses.llama = [engine('llama', true, {}, { standing: 'older', installedRelease: 'b10327', updateDownloads: true })];
    await reachEngines();

    await userEvent.click(await screen.findByRole('button', { name: 'Install' }));
    await vi.waitFor(() => expect(installs()).toHaveLength(1));
    expect(installs()[0][0]).toBe('/api/config/system/engines/stable_diffusion/install');
  });

  it('leaves a source build to the terminal, and says the CPU warning of what it installs', async () => {
    statuses.llama = [engine('llama', false, {}, { built: true, rebuilds: true, updateDownloads: false })];
    statuses.stable_diffusion = [engine('stable_diffusion', false, { download: 'Linux x64 (CPU)', warning: 'Images will be slow.' })];
    await reachEngines();

    expect(
      await screen.findByText('Built from source here before. Build it again from a terminal: gglib config llama update.'),
    ).toBeInTheDocument();
    expect(screen.getByText('Images will be slow.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    await vi.waitFor(() => expect(installs()).toHaveLength(1));
    expect(installs()[0][0]).toBe('/api/config/system/engines/stable_diffusion/install');
    expect(urls().some((url) => url.endsWith('/engines/llama/install'))).toBe(false);
  });

  it('names the command where no download fits, and installs nothing for it', async () => {
    statuses.llama = [engine('llama', false, { download: null, downloadUnavailable: 'No asset.' }, { updateDownloads: false })];
    statuses.stable_diffusion = [engine('stable_diffusion', true)];
    await reachEngines();

    expect(
      await screen.findByText('No asset. Build it from a terminal: gglib config llama install.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeInTheDocument();
  });

  it('settles an install whose stream ends without a result, and goes on to the next', async () => {
    await pressInstall();

    bodies.llama!.close();
    await vi.waitFor(() => expect(installs()).toHaveLength(2));
    send('stable_diffusion', { type: 'completed', version: 'master-948-228c707' });

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The connection to the install ended before it reported a result.',
    );
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
  });

  it('settles an install the daemon refuses outright, and goes on to the next', async () => {
    refused = 'llama';
    await reachEngines();
    await userEvent.click(await screen.findByRole('button', { name: 'Install' }));
    await vi.waitFor(() => expect(installs()).toHaveLength(2));
    send('stable_diffusion', { type: 'completed', version: 'master-948-228c707' });

    expect(await screen.findByRole('alert')).toHaveTextContent('llama.cpp did not install');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
  });
});
