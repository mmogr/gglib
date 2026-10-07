/**
 * The setup wizard's llama.cpp step.
 *
 * The install prompt in the app shell now reads the same stream and draws it
 * with the same `InstallProgress`, so this pins the wizard's side of what the
 * two share: the request its Install button makes, and how it shows progress,
 * completion and failure. The transport is the real one over a stubbed `fetch`.
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
import { INSTALL_PHASE_LABELS, type LlamaProgressEvent, type SetupStatus } from '../../../src/types/setup';
import { formatBytes, formatDuration, formatRate } from '../../../src/utils/format';

const encoder = new TextEncoder();

const STATUS: SetupStatus = {
  setupCompleted: false,
  llamaInstalled: false,
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

describe('the setup wizard\'s llama.cpp step', () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let body: ReadableStreamDefaultController<Uint8Array>;

  /** One frame of the install stream, as `install_event_to_sse` writes it. */
  const send = (event: LlamaProgressEvent) =>
    body.enqueue(encoder.encode(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`));

  beforeEach(() => {
    fetchMock = vi.fn(async (url: string) =>
      url.endsWith('/setup-status')
        ? new Response(JSON.stringify(STATUS), { status: 200, headers: { 'content-type': 'application/json' } })
        : new Response(new ReadableStream<Uint8Array>({ start: (controller) => void (body = controller) }), {
            status: 200,
            headers: { 'content-type': 'text/event-stream' },
          }),
    );
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => vi.unstubAllGlobals());

  /** Walk to the llama.cpp step and press Install. */
  async function pressInstall() {
    render(<SetupWizard onComplete={vi.fn()} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Get Started' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await userEvent.click(screen.getByRole('button', { name: 'Install' }));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
  }

  it('asks the daemon for its install stream and draws the phase, then the bytes as the daemon measured them', async () => {
    await pressInstall();

    const [url, init] = fetchMock.mock.calls[1] as [string, RequestInit];
    expect(url).toBe('/api/config/system/install-llama');
    expect(init.method).toBe('POST');

    send({ type: 'phase_started', phase: 'fetch_release' });
    expect(await screen.findByText(INSTALL_PHASE_LABELS.fetch_release)).toBeInTheDocument();

    send({ type: 'progress', downloaded: 5_000_000, total: 20_000_000, rate_bps: 2_500_000, eta_seconds: 6 });
    expect(await screen.findByText('25.0%')).toBeInTheDocument();
    expect(screen.getByText(`${formatBytes(5_000_000)} / ${formatBytes(20_000_000)}`)).toBeInTheDocument();
    expect(screen.getByText(formatRate(2_500_000))).toBeInTheDocument();
    expect(screen.getByText(`${formatDuration(6)} remaining`)).toBeInTheDocument();
    // No way forward or back while the install runs.
    expect(screen.queryByRole('button', { name: 'Skip' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });

  it('shows the engine as installed once the stream completes', async () => {
    await pressInstall();

    send({ type: 'completed', version: 'b6123' });

    expect(await screen.findByText('llama.cpp binaries are installed')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Continue' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows the daemon\'s own words when the install fails, and offers a retry', async () => {
    await pressInstall();

    send({ type: 'failed', message: 'Failed to install llama.cpp: no asset for this platform' });

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Installation failed');
    expect(alert).toHaveTextContent('Failed to install llama.cpp: no asset for this platform');
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Skip' })).toBeInTheDocument();
  });
});
