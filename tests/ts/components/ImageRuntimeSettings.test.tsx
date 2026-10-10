/**
 * Settings' image runtime section: the web face of `gglib config sd
 * install|status|uninstall`.
 *
 * The transport is the real one over a stubbed `fetch`, so what is pinned is
 * the request each action makes and what the section draws from the answer:
 * the status, the CPU build's warning, the install's progress named for
 * stable-diffusion.cpp, the command where no pre-built build fits, a removal
 * that asks first and does nothing when declined, and a removal that is not
 * offered while an image model runs.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

vi.mock('../../../src/services/platform', async (original) => ({
  ...(await original<typeof import('../../../src/services/platform')>()),
  appLogger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

const { confirm } = vi.hoisted(() => ({
  confirm: vi.fn<(options: { title: string }) => Promise<boolean>>(),
}));

vi.mock('../../../src/contexts/ConfirmContext', () => ({
  useConfirmContext: () => ({ confirm }),
}));

import { ImageRuntimeSettings } from '../../../src/components/SettingsModal/ImageRuntimeSettings';
import { resetClientCache, setApiSession } from '../../../src/services/transport/api/client';
import type { ImageRuntimeStatus, LlamaProgressEvent } from '../../../src/types/setup';

const encoder = new TextEncoder();
const CPU_WARNING = 'No GPU runtime gglib can use was found. Images will take minutes each.';

function status(overrides: Partial<ImageRuntimeStatus> = {}, installed = false): ImageRuntimeStatus {
  return {
    install: {
      installed,
      binaryPath: '/data/.sd/bin/sd-server',
      configPath: '/data/.sd/sd-config.json',
      pinnedRelease: 'master-948-228c707',
      installType: installed ? 'prebuilt' : null,
      release: installed ? 'master-948-228c707' : null,
      platform: installed ? 'macOS universal (Metal)' : null,
      installedAt: null,
      recordError: null,
      versionLine: installed ? 'stable-diffusion.cpp version unknown, commit 228c707' : null,
      commit: installed ? '228c707' : null,
    },
    prebuilt: 'macOS universal (Metal)',
    prebuiltUnavailable: null,
    warning: null,
    runningModel: null,
    installCommand: 'gglib config sd install',
    ...overrides,
  };
}

const json = (body: unknown, init: ResponseInit = { status: 200 }) =>
  new Response(JSON.stringify(body), { ...init, headers: { 'content-type': 'application/json' } });

/** The daemon's reply to the install request: a stream the test writes to. */
function installStream() {
  let body!: ReadableStreamDefaultController<Uint8Array>;
  const response = new Response(
    new ReadableStream<Uint8Array>({ start: (controller) => void (body = controller) }),
    { status: 200, headers: { 'content-type': 'text/event-stream' } },
  );
  return {
    response,
    send: (event: LlamaProgressEvent) =>
      body.enqueue(encoder.encode(`event: ${event.type}\ndata: ${JSON.stringify(event)}\n\n`)),
    close: () => body.close(),
  };
}

describe('the image runtime section', () => {
  let statuses: ImageRuntimeStatus[];
  let stream: ReturnType<typeof installStream>;
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    confirm.mockReset();
    confirm.mockResolvedValue(true);
    stream = installStream();
    statuses = [];
    fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith('/api/config/system/sd-status')) return json(statuses.shift() ?? status({}, true));
      if (url.endsWith('/api/config/system/install-sd')) return stream.response;
      if (url.endsWith('/api/config/system/uninstall-sd')) {
        return json({ wasInstalled: true, removedPaths: ['/data/.sd'] });
      }
      throw new Error(`unexpected request ${url}`);
    });
    vi.stubGlobal('fetch', fetchMock);
    resetClientCache();
    setApiSession('', undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const calls = (path: string) =>
    (fetchMock.mock.calls as [string, RequestInit | undefined][]).filter(([url]) => url.endsWith(path));

  it('offers the pre-built build with the CPU warning, and installs it with progress named for stable-diffusion.cpp', async () => {
    statuses = [status({ prebuilt: 'Linux x64 (CPU)', warning: CPU_WARNING })];
    render(<ImageRuntimeSettings />);

    expect(await screen.findByText(CPU_WARNING)).toBeInTheDocument();
    expect(screen.getByText('Not installed')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Install (Linux x64 (CPU))' }));

    await vi.waitFor(() => expect(calls('/install-sd')).toHaveLength(1));
    expect(calls('/install-sd')[0][1]?.method).toBe('POST');
    stream.send({ type: 'phase_started', phase: 'download' });
    expect(await screen.findByText('Downloading stable-diffusion.cpp binaries…')).toBeInTheDocument();

    stream.send({ type: 'completed', version: 'master-948-228c707' });
    stream.close();
    expect(await screen.findByText('stable-diffusion.cpp master-948-228c707 is installed.')).toBeInTheDocument();
    // The status is read again, and now reports the install.
    expect(await screen.findByText('Installed')).toBeInTheDocument();
  });

  it('says the daemon\'s words when the install fails', async () => {
    statuses = [status()];
    render(<ImageRuntimeSettings />);
    await userEvent.click(await screen.findByRole('button', { name: /^Install/ }));
    await vi.waitFor(() => expect(calls('/install-sd')).toHaveLength(1));

    stream.send({ type: 'failed', message: 'Failed to install stable-diffusion.cpp: no asset' });

    expect(await screen.findByText('Failed to install stable-diffusion.cpp: no asset')).toBeInTheDocument();
  });

  it('names the command where no pre-built build fits, and offers no install', async () => {
    statuses = [
      status({
        prebuilt: null,
        prebuiltUnavailable: 'stable-diffusion.cpp publishes no pre-built linux build for aarch64',
      }),
    ];
    render(<ImageRuntimeSettings />);

    expect(await screen.findByText(/publishes no pre-built linux build for aarch64/)).toBeInTheDocument();
    expect(screen.getByText('gglib config sd install')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Install/ })).not.toBeInTheDocument();
  });

  it('shows what is installed, and removes it after a confirm', async () => {
    statuses = [status({}, true), status()];
    render(<ImageRuntimeSettings />);

    expect(await screen.findByText('stable-diffusion.cpp version unknown, commit 228c707')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove the image runtime' }));

    expect(await screen.findByText('Removed /data/.sd.')).toBeInTheDocument();
    expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ title: 'Remove the image runtime?' }));
    expect(calls('/uninstall-sd')[0][1]?.method).toBe('POST');
  });

  it('removes nothing when the confirm is declined', async () => {
    confirm.mockResolvedValue(false);
    statuses = [status({}, true)];
    render(<ImageRuntimeSettings />);

    await userEvent.click(await screen.findByRole('button', { name: 'Remove the image runtime' }));

    await vi.waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls('/uninstall-sd')).toHaveLength(0);
    expect(screen.getByText('Installed')).toBeInTheDocument();
  });

  it('does not offer the removal while an image model runs on it', async () => {
    statuses = [status({ runningModel: 'flux-schnell' }, true)];
    render(<ImageRuntimeSettings />);

    expect(await screen.findByText('flux-schnell')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove the image runtime' })).toBeDisabled();
  });
});
