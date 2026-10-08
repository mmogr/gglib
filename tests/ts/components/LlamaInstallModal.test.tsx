/**
 * The llama.cpp install prompt runs the daemon's install.
 *
 * There were two installs. The setup wizard read the daemon's stream; this
 * prompt called a command only the desktop app had, so in a browser its
 * button closed the prompt having installed nothing. Now the prompt reads the
 * wizard's stream on both surfaces.
 *
 * The transport here is the real one over a stubbed `fetch`, so what is pinned
 * is the request the prompt makes and what it draws from the frames that come
 * back, once as the desktop app and once as a browser tab.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { LlamaInstallModal } from '../../../src/components/LlamaInstallModal';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import { INSTALL_PHASE_LABELS, type LlamaProgressEvent } from '../../../src/types/setup';
import { formatBytes, formatDuration, formatRate } from '../../../src/utils/format';
import { mockInvoke } from '../setup';

const encoder = new TextEncoder();

/** The daemon's reply to the install request: a stream the test writes to. */
function installStream() {
  let body!: ReadableStreamDefaultController<Uint8Array>;
  const response = new Response(
    new ReadableStream<Uint8Array>({ start: (controller) => void (body = controller) }),
    { status: 200, headers: { 'content-type': 'text/event-stream' } },
  );
  return {
    response,
    /** One frame, as `install_event_to_sse` writes it. */
    send: (event: LlamaProgressEvent) =>
      body.enqueue(encoder.encode(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`)),
    close: () => body.close(),
  };
}

/** Let whatever the stream last did reach the prompt: a full turn of the event loop. */
const settled = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)));

const METADATA = {
  expectedPath: '/data/bin/llama-server',
  suggestedCommand: 'gglib config llama install',
  reason: 'llama-server is not installed',
};

describe.each([
  ['the desktop app', 'http://127.0.0.1:9887'],
  ['a browser tab', ''],
])('the llama.cpp install prompt in %s', (_surface, daemon) => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let bridge: ReturnType<typeof vi.fn>;
  let stream: ReturnType<typeof installStream>;

  beforeEach(() => {
    stream = installStream();
    fetchMock = vi.fn(async () => stream.response);
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
    bridge = vi.fn().mockResolvedValue({ port: 9887, token: 'desk-token' });
    if (daemon) Object.assign(window, { __TAURI_INTERNALS__: { invoke: bridge } });
  });

  afterEach(() => {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    vi.unstubAllGlobals();
  });

  /** Open the prompt as the app shell does and press its install button. */
  async function pressInstall(onInstalled = vi.fn()) {
    const view = render(<LlamaInstallModal canDownload onSkip={vi.fn()} onInstalled={onInstalled} />);
    await userEvent.click(screen.getByRole('button', { name: 'Install llama.cpp' }));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());
    return view;
  }

  it('asks the daemon for the install stream the setup wizard reads, and calls no Tauri command for it', async () => {
    await pressInstall();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${daemon}/api/config/system/install-llama`);
    expect(init.method).toBe('POST');
    // The desktop app asks its shell where the daemon is, and nothing else.
    expect(bridge.mock.calls).toEqual(daemon ? [['get_embedded_api_info']] : []);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('draws each phase by name, then the bytes with the rate and the time left that the daemon measured', async () => {
    await pressInstall();

    stream.send({ type: 'phase_started', phase: 'fetch_release' });
    expect(await screen.findByText(INSTALL_PHASE_LABELS.fetch_release)).toBeInTheDocument();

    stream.send({ type: 'progress', downloaded: 5_000_000, total: 20_000_000, rate_bps: 2_500_000, eta_seconds: 6 });
    expect(await screen.findByText('25.0%')).toBeInTheDocument();
    expect(screen.getByText(`${formatBytes(5_000_000)} / ${formatBytes(20_000_000)}`)).toBeInTheDocument();
    expect(screen.getByText(formatRate(2_500_000))).toBeInTheDocument();
    expect(screen.getByText(`${formatDuration(6)} remaining`)).toBeInTheDocument();
    // Nothing closes the prompt or starts a second install while one runs.
    expect(screen.queryByRole('button', { name: 'Install llama.cpp' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeDisabled();
  });

  it('ends as complete, names the version installed, reports it once, and stays up to be read', async () => {
    const onInstalled = vi.fn();
    const onClose = vi.fn();
    render(<LlamaInstallModal canDownload onClose={onClose} onInstalled={onInstalled} />);
    await userEvent.click(screen.getByRole('button', { name: 'Install llama.cpp' }));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled());

    stream.send({ type: 'phase_started', phase: 'verify' });
    stream.send({ type: 'completed', version: 'b6123' });
    stream.close();

    expect(await screen.findByText('Installation complete')).toBeInTheDocument();
    expect(screen.getByText(/llama\.cpp b6123 is ready/)).toBeInTheDocument();
    // The stream closing after its result is not a second result.
    await settled();
    expect(screen.getByText('Installation complete')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install llama.cpp' })).not.toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledTimes(1);
    // It is over: nothing is still drawn as running, and the prompt can be closed again.
    expect(screen.queryByText('Preparing download…')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeEnabled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it('shows the daemon\'s own words when the install fails, and installs again when asked', async () => {
    const onInstalled = vi.fn();
    await pressInstall(onInstalled);

    stream.send({ type: 'failed', message: 'Failed to install llama.cpp: no asset for this platform' });
    stream.close();

    expect(await screen.findByRole('alert')).toHaveTextContent('Failed to install llama.cpp: no asset for this platform');
    await settled();
    expect(screen.getByRole('alert')).toHaveTextContent('Failed to install llama.cpp: no asset for this platform');
    expect(onInstalled).not.toHaveBeenCalled();

    stream = installStream();
    await userEvent.click(screen.getByRole('button', { name: 'Install llama.cpp' }));
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    stream.send({ type: 'completed', version: 'b6123' });
    expect(await screen.findByText('Installation complete')).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledTimes(1);
  });

  it('shows the daemon\'s refusal when the request itself is refused', async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: 'the daemon is shutting down', status: 503 }), {
        status: 503,
        headers: { 'content-type': 'application/json' },
      }),
    );
    await pressInstall();

    expect(await screen.findByRole('alert')).toHaveTextContent('the daemon is shutting down');
    expect(screen.getByRole('button', { name: 'Install llama.cpp' })).toBeEnabled();
  });

  it('stops installing, and says why, when the stream closes having reported no result', async () => {
    await pressInstall();

    stream.send({ type: 'phase_started', phase: 'download' });
    stream.close();

    expect(await screen.findByRole('alert')).toHaveTextContent(/ended before it reported a result/);
    expect(screen.getByRole('button', { name: 'Install llama.cpp' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Close dialog' })).toBeEnabled();
  });

  it('stops reading the stream when it is unmounted mid-install', async () => {
    const { unmount } = await pressInstall();
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(init.signal?.aborted).toBe(false);

    unmount();

    expect(init.signal?.aborted).toBe(true);
  });

  it('shows why the install state could not be read, until an install is tried', async () => {
    render(<LlamaInstallModal canDownload error="Failed to check llama status: offline" onSkip={vi.fn()} />);
    expect(screen.getByRole('alert')).toHaveTextContent('Failed to check llama status: offline');

    await userEvent.click(screen.getByRole('button', { name: 'Install llama.cpp' }));
    stream.send({ type: 'failed', message: 'no disk space' });

    await vi.waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('no disk space'));
  });

  it('offers no install where no prebuilt binary exists, only the way to build one', () => {
    const onSkip = vi.fn();
    render(<LlamaInstallModal canDownload={false} onSkip={onSkip} />);

    expect(screen.getByText(/gglib config llama install/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Install llama.cpp' })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'I understand' })).toBeInTheDocument();
  });

  describe('opened by a server start that found no llama-server', () => {
    it('installs over the same stream, shows its progress, and closes once it completes', async () => {
      const onClose = vi.fn();
      const onInstalled = vi.fn();
      render(<LlamaInstallModal metadata={METADATA} onClose={onClose} onInstalled={onInstalled} />);
      expect(screen.getByText(METADATA.expectedPath)).toBeInTheDocument();

      await userEvent.click(screen.getByRole('button', { name: 'Install now' }));
      await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      expect((fetchMock.mock.calls[0] as [string])[0]).toBe(`${daemon}/api/config/system/install-llama`);

      stream.send({ type: 'phase_started', phase: 'extract' });
      expect(await screen.findByText(INSTALL_PHASE_LABELS.extract)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Install now' })).toBeDisabled();
      expect(onClose).not.toHaveBeenCalled();

      stream.send({ type: 'completed', version: 'b6123' });

      await vi.waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
      expect(onInstalled).toHaveBeenCalledTimes(1);
    });

    it('stays open with the failure when the install fails', async () => {
      const onClose = vi.fn();
      render(<LlamaInstallModal metadata={METADATA} onClose={onClose} />);

      await userEvent.click(screen.getByRole('button', { name: 'Install now' }));
      await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
      stream.send({ type: 'failed', message: 'Pre-built binaries not available: unsupported platform' });

      expect(await screen.findByRole('alert')).toHaveTextContent('Pre-built binaries not available: unsupported platform');
      expect(screen.getByRole('button', { name: 'Install now' })).toBeEnabled();
      expect(onClose).not.toHaveBeenCalled();
    });
  });
});
