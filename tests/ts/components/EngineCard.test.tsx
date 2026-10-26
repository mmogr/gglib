/**
 * The engine card in System settings, once per engine: the web face of
 * `gglib config llama|sd install|status|uninstall`.
 *
 * The transport is the real one over a stubbed `fetch`, so what is pinned is
 * the request each action makes and what the card draws from the answer:
 * where the engine stands against its pin, the download with its CPU
 * warning, the install's progress named for the engine, the update and the
 * command where it is a source build, a removal that asks first, and no
 * action under a running model. Each engine's install state lives in a
 * module store that outlives the card, so each test loads the module afresh,
 * and two cards side by side keep their installs apart.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
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
vi.mock('../../../src/components/SettingsModal/DiagnosticsPanel', () => ({ DiagnosticsPanel: () => null }));
vi.mock('../../../src/components/SettingsModal/BuildInfo', () => ({ BuildInfo: () => null }));

import type { EngineStatus, LlamaProgressEvent, PinState, RuntimeKind } from '../../../src/types/setup';

const encoder = new TextEncoder();

/** What differs between the two engines' answers. */
const ENGINES = {
  llama: {
    product: 'llama.cpp',
    word: 'llama',
    folder: '.llama',
    server: 'llama-server',
    pin: 'b11429',
    older: 'b10327',
    download: 'macOS ARM64 (Metal)',
    model: 'qwen',
  },
  stable_diffusion: {
    product: 'stable-diffusion.cpp',
    word: 'sd',
    folder: '.sd',
    server: 'sd-server',
    pin: 'master-948-228c707',
    older: 'master-900-aaaaaaa',
    download: 'macOS universal (Metal)',
    model: 'flux-schnell',
  },
} as const;

const KINDS = Object.keys(ENGINES) as RuntimeKind[];

const CPU_WARNING = 'No GPU runtime gglib can use was found, so the CPU build will be slow.';

type Standing = 'missing' | 'atPin' | 'older';

/** `kind`'s status, standing as named, with `overrides` and `pin` laid over it. */
function status(
  kind: RuntimeKind,
  standing: Standing,
  overrides: Partial<EngineStatus> = {},
  pin: Partial<PinState> = {},
): EngineStatus {
  const e = ENGINES[kind];
  const installed = standing !== 'missing';
  const release = standing === 'older' ? e.older : e.pin;
  return {
    kind,
    installed,
    broken: null,
    binaryPath: `/data/${e.folder}/bin/${e.server}`,
    install: installed ? { built: false, release, platform: e.download, date: '2026-10-01T00:00:00Z' } : null,
    recordError: null,
    reports: installed ? `${e.server} reports ${release}` : null,
    pin: {
      standing,
      installedRelease: installed ? release : null,
      targetRelease: e.pin,
      built: installed ? false : null,
      rebuilds: false,
      updateDownloads: standing !== 'atPin',
      ...pin,
    },
    download: e.download,
    downloadUnavailable: null,
    warning: null,
    serving: null,
    installCommand: `gglib config ${e.word} install`,
    updateCommand: `gglib config ${e.word} update`,
    ...overrides,
  };
}

const json = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

/** The daemon's reply to an install request: a stream the test writes to. */
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

let statuses: Record<RuntimeKind, EngineStatus[]>;
let streams: Record<RuntimeKind, ReturnType<typeof installStream>>;
let fetchMock: ReturnType<typeof vi.fn>;
let EngineCard: FC<{ kind: RuntimeKind }>;
let SystemSettings: FC;

beforeEach(async () => {
  vi.resetModules();
  confirm.mockReset();
  confirm.mockResolvedValue(true);
  streams = { llama: installStream(), stable_diffusion: installStream() };
  statuses = { llama: [], stable_diffusion: [] };
  fetchMock = vi.fn(async (url: string) => {
    for (const kind of KINDS) {
      const base = `/api/config/system/engines/${kind}`;
      if (url.endsWith(base)) return json(statuses[kind].shift() ?? status(kind, 'atPin'));
      if (url.endsWith(`${base}/install`)) return streams[kind].response;
      if (url.endsWith(`${base}/uninstall`)) {
        return json({ wasInstalled: true, removedPaths: [`/data/${ENGINES[kind].folder}`] });
      }
    }
    throw new Error(`unexpected request ${url}`);
  });
  vi.stubGlobal('fetch', fetchMock);
  const client = await import('../../../src/services/transport/api/client');
  client.resetClientCache();
  client.setApiSession('', undefined);
  ({ EngineCard } = await import('../../../src/components/SettingsModal/EngineCard'));
  ({ SystemSettings } = await import('../../../src/components/SettingsModal/SystemSettings'));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

const calls = (path: string) =>
  (fetchMock.mock.calls as [string, RequestInit | undefined][]).filter(([url]) => url.endsWith(path));

describe.each(KINDS)('the %s card', (kind) => {
  const e = ENGINES[kind];
  const base = `/engines/${kind}`;

  it('offers the download with the CPU warning, and installs it with progress named for the engine', async () => {
    statuses[kind] = [status(kind, 'missing', { download: 'Linux x64 (CPU)', warning: CPU_WARNING })];
    render(<EngineCard kind={kind} />);

    expect(await screen.findByText(CPU_WARNING)).toBeInTheDocument();
    expect(screen.getByText('Not installed')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Install (Linux x64 (CPU))' }));

    await vi.waitFor(() => expect(calls(`${base}/install`)).toHaveLength(1));
    expect(calls(`${base}/install`)[0][1]?.method).toBe('POST');
    streams[kind].send({ type: 'phase_started', phase: 'download' });
    expect(await screen.findByText(`Downloading ${e.product} binaries…`)).toBeInTheDocument();

    streams[kind].send({ type: 'completed', version: e.pin });
    streams[kind].close();
    expect(await screen.findByText(`${e.product} ${e.pin} is installed.`)).toBeInTheDocument();
    // The status is read again, and now reports the install.
    expect(await screen.findByText('Installed')).toBeInTheDocument();
  });

  it('says the CPU warning only where it offers the download', async () => {
    statuses[kind] = [status(kind, 'atPin', { warning: CPU_WARNING })];
    const atPin = render(<EngineCard kind={kind} />);

    expect(await screen.findByText(`Pinned to ${e.pin}, which is the one installed.`)).toBeInTheDocument();
    expect(screen.queryByText(CPU_WARNING)).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Install/ })).not.toBeInTheDocument();
    atPin.unmount();

    statuses[kind] = [status(kind, 'older', { warning: CPU_WARNING })];
    render(<EngineCard kind={kind} />);
    expect(await screen.findByRole('button', { name: `Update to ${e.pin}` })).toBeInTheDocument();
    expect(screen.getByText(CPU_WARNING)).toBeInTheDocument();
  });

  it('sends a source build whose binary is gone to the terminal, not to a download', async () => {
    statuses[kind] = [status(kind, 'missing', {}, { built: true, rebuilds: true, updateDownloads: false })];
    render(<EngineCard kind={kind} />);

    expect(
      await screen.findByText(/was built from source here before, so it is built again from a terminal/),
    ).toBeInTheDocument();
    expect(screen.getByText(`gglib config ${e.word} update`)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Install/ })).not.toBeInTheDocument();
  });

  it('says where an older engine stands, and updates it to the pin after a confirm', async () => {
    statuses[kind] = [status(kind, 'older')];
    render(<EngineCard kind={kind} />);

    expect(
      await screen.findByText(`Pinned to ${e.pin}. The one installed, ${e.older}, is older.`),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: `Update to ${e.pin}` }));

    expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ title: `Update ${e.product} to ${e.pin}?` }));
    await vi.waitFor(() => expect(calls(`${base}/install`)).toHaveLength(1));
    streams[kind].send({ type: 'completed', version: e.pin });
    streams[kind].close();

    expect(await screen.findByText(`Pinned to ${e.pin}, which is the one installed.`)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
  });

  it('updates nothing when the confirm is declined', async () => {
    confirm.mockResolvedValue(false);
    statuses[kind] = [status(kind, 'older')];
    render(<EngineCard kind={kind} />);

    await userEvent.click(await screen.findByRole('button', { name: `Update to ${e.pin}` }));

    await vi.waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls(`${base}/install`)).toHaveLength(0);
  });

  it('reads the status back after an install that finished while the tab was away', async () => {
    statuses[kind] = [status(kind, 'older'), status(kind, 'older')];
    const first = render(<EngineCard kind={kind} />);
    await userEvent.click(await screen.findByRole('button', { name: `Update to ${e.pin}` }));
    await vi.waitFor(() => expect(calls(`${base}/install`)).toHaveLength(1));
    first.unmount();
    render(<EngineCard kind={kind} />);
    await vi.waitFor(() => expect(calls(base)).toHaveLength(2));

    streams[kind].send({ type: 'completed', version: e.pin });
    streams[kind].close();

    expect(await screen.findByText(`Pinned to ${e.pin}, which is the one installed.`)).toBeInTheDocument();
    expect(calls(base)).toHaveLength(3);
  });

  it("says the daemon's words when the install fails", async () => {
    statuses[kind] = [status(kind, 'missing')];
    render(<EngineCard kind={kind} />);
    await userEvent.click(await screen.findByRole('button', { name: /^Install/ }));
    await vi.waitFor(() => expect(calls(`${base}/install`)).toHaveLength(1));

    streams[kind].send({ type: 'failed', message: `Failed to install ${e.product}: no asset` });

    expect(await screen.findByText(`Failed to install ${e.product}: no asset`)).toBeInTheDocument();
  });

  it('offers neither the update nor the removal while a model runs on it', async () => {
    statuses[kind] = [status(kind, 'older', { serving: e.model })];
    render(<EngineCard kind={kind} />);

    const update = await screen.findByRole('button', { name: `Update to ${e.pin}` });
    expect(update).toBeDisabled();
    expect(update).toHaveAttribute('title', `Stop ${e.model} first.`);
    expect(screen.getByRole('button', { name: `Remove ${e.product}` })).toBeDisabled();
    expect(screen.getByText(e.model)).toBeInTheDocument();
  });

  it('sends an update the command line builds to the terminal', async () => {
    statuses[kind] = [status(kind, 'older', {}, { built: true, rebuilds: true, updateDownloads: false })];
    render(<EngineCard kind={kind} />);

    expect(
      await screen.findByText(`The update is a source build, run from a terminal: gglib config ${e.word} update.`),
    ).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
  });

  it('names the command where no download fits, and offers no install', async () => {
    statuses[kind] = [
      status(
        kind,
        'missing',
        { download: null, downloadUnavailable: 'No download fits linux aarch64.' },
        { updateDownloads: false },
      ),
    ];
    render(<EngineCard kind={kind} />);

    expect(await screen.findByText(/No download fits linux aarch64\. Build it from source with/)).toBeInTheDocument();
    expect(screen.getByText(`gglib config ${e.word} install`)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Install/ })).not.toBeInTheDocument();
  });

  it('shows what is installed, and removes it after a confirm', async () => {
    statuses[kind] = [status(kind, 'atPin'), status(kind, 'missing')];
    render(<EngineCard kind={kind} />);

    expect(await screen.findByText(`${e.server} reports ${e.pin}`)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: `Remove ${e.product}` }));

    expect(await screen.findByText(`Removed /data/${e.folder}.`)).toBeInTheDocument();
    expect(confirm).toHaveBeenCalledWith(expect.objectContaining({ title: `Remove ${e.product}?` }));
    expect(calls(`${base}/uninstall`)[0][1]?.method).toBe('POST');
    expect(await screen.findByText('Not installed')).toBeInTheDocument();
  });

  it('removes nothing when the confirm is declined', async () => {
    confirm.mockResolvedValue(false);
    statuses[kind] = [status(kind, 'atPin')];
    render(<EngineCard kind={kind} />);

    await userEvent.click(await screen.findByRole('button', { name: `Remove ${e.product}` }));

    await vi.waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(calls(`${base}/uninstall`)).toHaveLength(0);
    expect(screen.getByText('Installed')).toBeInTheDocument();
  });
});

describe('the engine card', () => {
  it('says an install that does not work is not working, and why', async () => {
    statuses.llama = [status('llama', 'atPin', { broken: 'llama-server exited: missing libggml' })];
    render(<EngineCard kind="llama" />);

    expect(await screen.findByText('Installed, not working')).toBeInTheDocument();
    expect(screen.getByText('llama-server exited: missing libggml')).toBeInTheDocument();
  });

  it("says a source build is one, by its record's words", async () => {
    statuses.llama = [
      status('llama', 'atPin', {
        install: { built: true, release: 'b11429', platform: 'Metal', date: '2026-10-25T10:28:54Z' },
      }),
    ];
    render(<EngineCard kind="llama" />);

    expect(await screen.findByText('Built from')).toBeInTheDocument();
    expect(screen.getByText('Acceleration')).toBeInTheDocument();
    expect(screen.getByText('2026-10-25')).toBeInTheDocument();
  });

  it("shows one engine's install on that engine's card only", async () => {
    statuses.llama = [status('llama', 'older')];
    statuses.stable_diffusion = [status('stable_diffusion', 'older')];
    render(<SystemSettings />);

    const llama = within(await screen.findByRole('region', { name: 'llama.cpp' }));
    const sd = within(await screen.findByRole('region', { name: 'stable-diffusion.cpp' }));
    await userEvent.click(await llama.findByRole('button', { name: 'Update to b11429' }));
    await vi.waitFor(() => expect(calls('/engines/llama/install')).toHaveLength(1));
    streams.llama.send({ type: 'phase_started', phase: 'download' });

    expect(await llama.findByText('Downloading llama.cpp binaries…')).toBeInTheDocument();
    expect(sd.queryByText(/^Downloading/)).not.toBeInTheDocument();
    expect(sd.getByRole('button', { name: 'Update to master-948-228c707' })).not.toBeDisabled();
    expect(calls('/engines/stable_diffusion/install')).toHaveLength(0);
  });
});
