/**
 * Settings' llama.cpp section: where llama.cpp stands against its pinned
 * release, and the update, which is the daemon's install of the pin.
 *
 * The transport is the real one over a stubbed `fetch`. The other sections
 * of the tab have tests of their own and are stubbed out. The install state
 * lives in a module store that outlives the component, so each test loads
 * the module afresh.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { FC } from 'react';

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
vi.mock('../../../src/components/SettingsModal/ImageRuntimeSettings', () => ({ ImageRuntimeSettings: () => null }));
vi.mock('../../../src/components/SettingsModal/DiagnosticsPanel', () => ({ DiagnosticsPanel: () => null }));
vi.mock('../../../src/components/SettingsModal/BuildInfo', () => ({ BuildInfo: () => null }));

import type { LlamaProgressEvent, LlamaStatus, PinState } from '../../../src/types/setup';

const encoder = new TextEncoder();

function status(pin: Partial<PinState>, serving: string | null = null): LlamaStatus {
  return {
    installed: true,
    binaryPath: '/data/.llama/bin/llama-server',
    configPath: '/data/.llama/llama-config.json',
    healthy: true,
    healthError: null,
    build: null,
    buildError: null,
    prebuilt: { version: 'b10327', platform: 'macOS ARM64 (Metal)', installedAt: '2026-10-01T00:00:00Z' },
    runtime: null,
    pin: {
      standing: 'older',
      installedRelease: 'b10327',
      targetRelease: 'b11429',
      built: false,
      rebuilds: false,
      updateDownloads: true,
      ...pin,
    },
    serving,
  };
}

const AT_PIN = status({ standing: 'atPin', installedRelease: 'b11429', updateDownloads: false });

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

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

describe('the llama.cpp section of System settings', () => {
  let statuses: LlamaStatus[];
  let stream: ReturnType<typeof installStream>;
  let fetchMock: ReturnType<typeof vi.fn>;
  let SystemSettings: FC;

  beforeEach(async () => {
    vi.resetModules();
    confirm.mockReset();
    confirm.mockResolvedValue(true);
    stream = installStream();
    statuses = [];
    fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith('/api/config/system/llama-status')) return json(statuses.shift() ?? AT_PIN);
      if (url.endsWith('/api/config/system/install-llama')) return stream.response;
      throw new Error(`unexpected request ${url}`);
    });
    vi.stubGlobal('fetch', fetchMock);
    const client = await import('../../../src/services/transport/api/client');
    client.resetClientCache();
    client.setApiSession('', undefined);
    ({ SystemSettings } = await import('../../../src/components/SettingsModal/SystemSettings'));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  const calls = (path: string) =>
    (fetchMock.mock.calls as [string][]).filter(([url]) => url.endsWith(path));

  it('says where llama.cpp stands, and updates an older one to the pin with progress', async () => {
    statuses = [status({})];
    render(<SystemSettings />);

    expect(await screen.findByText('Pinned to b11429. The one installed, b10327, is older.')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Update to b11429' }));

    expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ title: 'Update llama.cpp to b11429?' }));
    await vi.waitFor(() => expect(calls('/install-llama')).toHaveLength(1));
    stream.send({ type: 'phase_started', phase: 'download' });
    expect(await screen.findByText('Downloading llama.cpp binaries…')).toBeInTheDocument();
    stream.send({ type: 'completed', version: 'b11429' });
    stream.close();

    expect(await screen.findByText('llama.cpp b11429 is installed.')).toBeInTheDocument();
    // The status is read again, and now reports the pin.
    expect(await screen.findByText('Pinned to b11429, which is the one installed.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
  });

  it('reads the status back after an install that finished while the tab was away', async () => {
    statuses = [status({}), status({})];
    const first = render(<SystemSettings />);
    await userEvent.click(await screen.findByRole('button', { name: 'Update to b11429' }));
    await vi.waitFor(() => expect(calls('/install-llama')).toHaveLength(1));
    first.unmount();
    render(<SystemSettings />);
    await vi.waitFor(() => expect(calls('/llama-status')).toHaveLength(2));

    stream.send({ type: 'completed', version: 'b11429' });
    stream.close();

    expect(await screen.findByText('Pinned to b11429, which is the one installed.')).toBeInTheDocument();
    expect(calls('/llama-status')).toHaveLength(3);
  });

  it('says the daemon\'s words when the update is refused', async () => {
    statuses = [status({})];
    render(<SystemSettings />);
    await userEvent.click(await screen.findByRole('button', { name: 'Update to b11429' }));
    await vi.waitFor(() => expect(calls('/install-llama')).toHaveLength(1));

    stream.send({
      type: 'failed',
      message: 'conflict: qwen is running on llama-server. Stop it before installing llama.cpp.',
    });

    expect(await screen.findByText(/Stop it before installing llama\.cpp/)).toBeInTheDocument();
  });

  it('offers neither the update nor the removal while a chat model runs on llama-server', async () => {
    statuses = [status({}, 'qwen')];
    render(<SystemSettings />);

    const update = await screen.findByRole('button', { name: 'Update to b11429' });
    expect(update).toBeDisabled();
    expect(update).toHaveAttribute('title', 'Stop qwen first.');
    // Nor the removal, which the daemon would refuse.
    expect(screen.getByRole('button', { name: 'Uninstall llama.cpp' })).toBeDisabled();
  });

  it('sends an update the command line builds to the terminal', async () => {
    statuses = [status({ built: true, rebuilds: true, updateDownloads: false })];
    render(<SystemSettings />);

    expect(
      await screen.findByText('The update is a source build, run from a terminal: gglib config llama update.'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
  });
});
