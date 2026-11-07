/**
 * Settings › Engines through the real routes: each engine's row in its
 * words (the release, the download or acceleration, prebuilt or built from
 * source, the state); Install; Update to the pin asked first, nothing sent
 * when declined; every action off while a model runs on it, saying which to
 * stop (ST-11); ⋯ Reinstall over a download only, ⋯ Remove… asked first;
 * ▸ Details from the install record; a source build sent to the terminal;
 * the CPU warning only where an install is offered; an install that fails
 * in the daemon's words, and one whose stream closes with no result ending
 * with a reason (SU-1); the two rows' installs apart; the row read back
 * after an install that finished while Engines was closed; Faster downloads both
 * ways, and the command where there is no Python.
 *
 * The setup routes are a fake whose install streams the test drives. Each
 * engine's install lives in a module store that outlives its row, so each
 * test loads the app afresh.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import type { EngineStatus, LlamaProgressEvent, PinState, RuntimeKind, SetupStatus } from '../../../src/types/setup';
import type { RenderedApp } from './harness';
import { appSettings } from '../fixtures/settings';

interface Stream {
  onEvent: (event: LlamaProgressEvent) => void;
  onError: (error: string) => void;
  onClose?: () => void;
}

const fake = vi.hoisted(() => ({
  statuses: { llama: [], stable_diffusion: [] } as Record<string, unknown[]>,
  streams: { llama: [], stable_diffusion: [] } as Record<string, Stream[]>,
  setup: { fastDownloadReady: false, pythonAvailable: true },
  uninstalled: [] as string[],
  enabled: 0,
  disabled: 0,
  refuseEnable: null as string | null,
  refuseUninstall: null as string | null,
  refuseSetupStatus: null as string | null,
  /** While set, a set up or a turn off waits for it. */
  hold: null as Promise<void> | null,
}));

vi.mock('../../../src/services/transport/api/setup', () => ({
  getEngineStatus: vi.fn(async (kind: string) => {
    const queue = fake.statuses[kind];
    const next = queue.length > 1 ? queue.shift() : queue[0];
    if (next instanceof Error) throw next;
    return next;
  }),
  streamEngineInstall: vi.fn((kind: string, onEvent, onError, onClose) => {
    fake.streams[kind].push({ onEvent, onError, onClose });
    return () => {};
  }),
  uninstallEngine: vi.fn(async (kind: string) => {
    if (fake.refuseUninstall) throw new Error(fake.refuseUninstall);
    fake.uninstalled.push(kind);
    return { wasInstalled: true, removedPaths: [`/data/.${kind === 'llama' ? 'llama' : 'sd'}`] };
  }),
  getSetupStatus: vi.fn(async () => {
    if (fake.refuseSetupStatus) throw new Error(fake.refuseSetupStatus);
    return { ...fake.setup };
  }),
  enableFastDownloads: vi.fn(async () => {
    fake.enabled += 1;
    if (fake.hold) await fake.hold;
    if (fake.refuseEnable) throw new Error(fake.refuseEnable);
    fake.setup.fastDownloadReady = true;
  }),
  disableFastDownloads: vi.fn(async () => {
    fake.disabled += 1;
    if (fake.hold) await fake.hold;
    fake.setup.fastDownloadReady = false;
    return { removed: true };
  }),
}));
vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getTransport: () => ({ getSettings: async () => appSettings(), subscribe: () => () => {}, onEventStreamOpen: () => () => {} }),
}));
vi.mock('../../../src/services/transport/api/settings', () => ({
  getSettings: async () => appSettings(),
  updateSettings: vi.fn(),
  installProfileTemplates: vi.fn(),
}));
vi.mock('../../../src/places/settings/DiagnosticsPanel', () => ({ DiagnosticsPanel: () => <p>system checks</p> }));
vi.mock('../../../src/places/library/LibraryPlace', () => ({ default: () => <main aria-label="Library" /> }));

const ENGINES = {
  llama: {
    product: 'llama.cpp',
    word: 'llama',
    server: 'llama-server',
    pin: 'b11429',
    older: 'b10327',
    download: 'macOS ARM64 (Metal)',
    model: 'qwen',
  },
  stable_diffusion: {
    product: 'stable-diffusion.cpp',
    word: 'sd',
    server: 'sd-server',
    pin: 'master-948-228c707',
    older: 'master-900-aaaaaaa',
    download: 'macOS universal (Metal)',
    model: 'flux-schnell',
  },
} as const;

const KINDS = Object.keys(ENGINES) as RuntimeKind[];
const CPU_WARNING = 'No GPU runtime gglib can use was found, so the CPU build will be slow.';

/** `kind`'s status, standing as named, with `overrides` and `pin` laid over it. */
function status(
  kind: RuntimeKind,
  standing: 'missing' | 'atPin' | 'older',
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
    binaryPath: `/data/.${e.word}/bin/${e.server}`,
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

/** A source build of `kind` at its pin. */
const built = (kind: RuntimeKind) =>
  status(
    kind,
    'atPin',
    { install: { built: true, release: ENGINES[kind].pin, platform: 'Metal', date: '2026-10-25T10:28:54Z' } },
    { built: true, rebuilds: true },
  );

let renderApp: (path: string) => RenderedApp;

beforeEach(async () => {
  vi.resetModules();
  fake.statuses = { llama: [status('llama', 'atPin')], stable_diffusion: [status('stable_diffusion', 'atPin')] };
  fake.streams = { llama: [], stable_diffusion: [] };
  fake.setup = { fastDownloadReady: false, pythonAvailable: true } as Pick<SetupStatus, 'fastDownloadReady' | 'pythonAvailable'>;
  fake.uninstalled = [];
  fake.enabled = 0;
  fake.disabled = 0;
  fake.refuseEnable = null;
  fake.refuseUninstall = null;
  fake.refuseSetupStatus = null;
  fake.hold = null;
  // A fresh load of the whole app, which a busy machine can take a while over.
  ({ renderApp } = await import('./harness'));
}, 30_000);

const row = async (product: string) => within(await screen.findByRole('region', { name: product }));
const dialog = async () => within(await screen.findByRole('dialog'));

async function openMore(product: string) {
  const r = await row(product);
  await userEvent.click(r.getByRole('button', { name: `More for ${product}` }));
  return r;
}

describe.each(KINDS)('the %s row', (kind) => {
  const e = ENGINES[kind];

  it('says what is installed in one line, and its state', async () => {
    renderApp('/settings/engines');
    const r = await row(e.product);
    expect(await r.findByText(`${e.pin} · ${e.download} · prebuilt`)).toBeInTheDocument();
    expect(r.getByText('Installed')).toBeInTheDocument();
    expect(r.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
    expect(r.queryByRole('button', { name: 'Install' })).not.toBeInTheDocument();
  });

  it('offers the install where nothing is, with the pin and the download, and installs it with progress', async () => {
    fake.statuses[kind] = [status(kind, 'missing', { warning: CPU_WARNING }), status(kind, 'atPin')];
    renderApp('/settings/engines');
    const r = await row(e.product);

    expect(await r.findByText(`Pinned to ${e.pin} · ${e.download}`)).toBeInTheDocument();
    expect(r.getByText('Not installed')).toBeInTheDocument();
    expect(r.getByText(CPU_WARNING)).toBeInTheDocument();
    await userEvent.click(r.getByRole('button', { name: 'Install' }));

    expect(fake.streams[kind]).toHaveLength(1);
    fake.streams[kind][0].onEvent({ type: 'phase_started', phase: 'download' });
    expect(await r.findByText(`Downloading ${e.product} binaries…`)).toBeInTheDocument();
    fake.streams[kind][0].onEvent({ type: 'completed', version: e.pin });
    expect(await r.findByText(`${e.product} ${e.pin} is installed.`)).toBeInTheDocument();
    expect(await r.findByText('Installed')).toBeInTheDocument();
  });

  it('updates to the pin only after a confirm, and reads the row again after', async () => {
    fake.statuses[kind] = [status(kind, 'older', { warning: CPU_WARNING }), status(kind, 'atPin')];
    renderApp('/settings/engines');
    const r = await row(e.product);
    expect(await r.findByText(`${e.older} · ${e.download} · prebuilt`)).toBeInTheDocument();
    expect(r.getByText(CPU_WARNING)).toBeInTheDocument();

    await userEvent.click(r.getByRole('button', { name: `Update to ${e.pin}…` }));
    const asked = await dialog();
    expect(asked.getByText(`Update ${e.product} to ${e.pin}?`)).toBeInTheDocument();
    await userEvent.click(asked.getByRole('button', { name: 'Update' }));

    await waitFor(() => expect(fake.streams[kind]).toHaveLength(1));
    fake.streams[kind][0].onEvent({ type: 'completed', version: e.pin });
    expect(await r.findByText(`${e.pin} · ${e.download} · prebuilt`)).toBeInTheDocument();
    expect(r.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
  });

  it('updates nothing when the confirm is declined', async () => {
    fake.statuses[kind] = [status(kind, 'older')];
    renderApp('/settings/engines');
    await userEvent.click(await (await row(e.product)).findByRole('button', { name: `Update to ${e.pin}…` }));
    await userEvent.click((await dialog()).getByRole('button', { name: 'Cancel' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(fake.streams[kind]).toHaveLength(0);
  });

  it('offers nothing while a model runs on it, and says which to stop', async () => {
    fake.statuses[kind] = [status(kind, 'older', { serving: e.model })];
    renderApp('/settings/engines');
    const r = await row(e.product);

    const update = await r.findByRole('button', { name: `Update to ${e.pin}…` });
    expect(update).toBeDisabled();
    expect(update).toHaveAttribute('title', `Stop ${e.model} first.`);
    expect(r.getByText(`Stop ${e.model} first.`)).toBeInTheDocument();
    await openMore(e.product);
    expect(r.getByRole('menuitem', { name: 'Reinstall' })).toBeDisabled();
    expect(r.getByRole('menuitem', { name: 'Reinstall' })).toHaveAttribute('title', `Stop ${e.model} first.`);
    expect(r.getByRole('menuitem', { name: 'Remove…' })).toBeDisabled();
    expect(r.getByRole('menuitem', { name: 'Remove…' })).toHaveAttribute('title', `Stop ${e.model} first.`);
    await userEvent.click(r.getByRole('button', { name: 'Details' }));
    expect(r.getByText('Serving').nextSibling).toHaveTextContent(new RegExp(`^${e.model}$`));
  });

  it('reinstalls a download at once, and removes it after a confirm', async () => {
    fake.statuses[kind] = [status(kind, 'atPin'), status(kind, 'atPin'), status(kind, 'missing')];
    renderApp('/settings/engines');
    let r = await openMore(e.product);
    await userEvent.click(await r.findByRole('menuitem', { name: 'Reinstall' }));
    expect(fake.streams[kind]).toHaveLength(1);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    fake.streams[kind][0].onEvent({ type: 'completed', version: e.pin });
    expect(await r.findByText(`${e.product} ${e.pin} is installed.`)).toBeInTheDocument();

    r = await openMore(e.product);
    await userEvent.click(r.getByRole('menuitem', { name: 'Remove…' }));
    const asked = await dialog();
    expect(asked.getByText(`Remove ${e.product}?`)).toBeInTheDocument();
    await userEvent.click(asked.getByRole('button', { name: 'Remove' }));

    expect(await r.findByText(`Removed /data/.${e.word}.`)).toBeInTheDocument();
    expect(fake.uninstalled).toEqual([kind]);
    expect(await r.findByText('Not installed')).toBeInTheDocument();
  });

  it('removes nothing when the confirm is declined', async () => {
    renderApp('/settings/engines');
    const r = await openMore(e.product);
    await userEvent.click(r.getByRole('menuitem', { name: 'Remove…' }));
    await userEvent.click((await dialog()).getByRole('button', { name: 'Cancel' }));

    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(fake.uninstalled).toEqual([]);
    expect(r.getByText('Installed')).toBeInTheDocument();
  });

  it('opens Details on the install record, the binary and where it stands', async () => {
    renderApp('/settings/engines');
    const r = await row(e.product);
    expect(r.queryByText('Binary reports')).not.toBeInTheDocument();
    await userEvent.click(await r.findByRole('button', { name: 'Details' }));

    expect(r.getByText('Release').nextSibling).toHaveTextContent(e.pin);
    expect(r.getByText('Download').nextSibling).toHaveTextContent(e.download);
    expect(r.getByText('Downloaded').nextSibling).toHaveTextContent(/^2026-10-01$/);
    expect(r.getByText('Binary').nextSibling).toHaveTextContent(`/data/.${e.word}/bin/${e.server}`);
    expect(r.getByText('Binary reports').nextSibling).toHaveTextContent(`${e.server} reports ${e.pin}`);
    expect(r.getByText(`Pinned to ${e.pin}, which is the one installed.`)).toBeInTheDocument();
    expect(r.queryByText('Serving')).not.toBeInTheDocument();
  });

  it('says a source build is one, offers no Reinstall over it, and sends its update to the terminal', async () => {
    fake.statuses[kind] = [
      status(
        kind,
        'older',
        { install: { built: true, release: e.older, platform: 'Metal', date: '2026-10-25T10:28:54Z' } },
        { built: true, rebuilds: true, updateDownloads: false },
      ),
    ];
    renderApp('/settings/engines');
    const r = await row(e.product);

    expect(await r.findByText(`${e.older} · Metal · built from source`)).toBeInTheDocument();
    expect(r.getByText(`The update is a source build, run from a terminal: gglib config ${e.word} update.`)).toBeInTheDocument();
    expect(r.queryByRole('button', { name: /^Update to/ })).not.toBeInTheDocument();
    await openMore(e.product);
    expect(r.queryByRole('menuitem', { name: 'Reinstall' })).not.toBeInTheDocument();
    expect(r.getByRole('menuitem', { name: 'Remove…' })).toBeEnabled();
  });

  it("says the daemon's words when the install fails", async () => {
    fake.statuses[kind] = [status(kind, 'missing')];
    renderApp('/settings/engines');
    await userEvent.click(await (await row(e.product)).findByRole('button', { name: 'Install' }));
    fake.streams[kind][0].onEvent({ type: 'failed', message: `Failed to install ${e.product}: no asset` });

    expect(await screen.findByText(`Failed to install ${e.product}: no asset`)).toBeInTheDocument();
  });

  it('ends an install whose stream closed with no result, with a reason (SU-1)', async () => {
    fake.statuses[kind] = [status(kind, 'missing')];
    renderApp('/settings/engines');
    const r = await row(e.product);
    await userEvent.click(await r.findByRole('button', { name: 'Install' }));
    fake.streams[kind][0].onEvent({ type: 'phase_started', phase: 'download' });
    expect(await r.findByText(`Downloading ${e.product} binaries…`)).toBeInTheDocument();

    fake.streams[kind][0].onClose?.();

    expect(
      await r.findByText(
        'The connection to the install ended before it reported a result. It may still be running: open Settings › Engines again to check.',
      ),
    ).toBeInTheDocument();
    expect(r.queryByText(`Downloading ${e.product} binaries…`)).not.toBeInTheDocument();
    expect(r.getByRole('button', { name: 'Install' })).toBeEnabled();
  });
});

describe('the engine rows', () => {
  it('sends a source build whose binary is gone to the terminal, not to a download', async () => {
    fake.statuses.llama = [status('llama', 'missing', {}, { built: true, rebuilds: true, updateDownloads: false })];
    renderApp('/settings/engines');
    const r = await row('llama.cpp');

    expect(await r.findByText(/was built from source here before, so it is built again from a terminal/)).toBeInTheDocument();
    expect(r.getByText('gglib config llama update')).toBeInTheDocument();
    expect(r.queryByRole('button', { name: 'Install' })).not.toBeInTheDocument();
  });

  it('names the command where no download fits, and offers no install', async () => {
    fake.statuses.llama = [
      status('llama', 'missing', { download: null, downloadUnavailable: 'No download fits linux aarch64.' }, { updateDownloads: false }),
    ];
    renderApp('/settings/engines');
    const r = await row('llama.cpp');

    expect(await r.findByText(/No download fits linux aarch64\. Build it from source with/)).toBeInTheDocument();
    expect(r.getByText('gglib config llama install')).toBeInTheDocument();
    expect(r.queryByRole('button', { name: 'Install' })).not.toBeInTheDocument();
  });

  it('says an install that does not work is not working, and why', async () => {
    fake.statuses.llama = [status('llama', 'atPin', { broken: 'llama-server exited: missing libggml' })];
    renderApp('/settings/engines');
    const r = await row('llama.cpp');

    expect(await r.findByText('Installed, not working')).toBeInTheDocument();
    expect(r.getByText('llama-server exited: missing libggml')).toBeInTheDocument();
  });

  it("names a source build's record in its own words in Details", async () => {
    fake.statuses.llama = [built('llama')];
    renderApp('/settings/engines');
    const r = await row('llama.cpp');
    await userEvent.click(await r.findByRole('button', { name: 'Details' }));

    expect(r.getByText('Built from').nextSibling).toHaveTextContent('b11429');
    expect(r.getByText('Acceleration').nextSibling).toHaveTextContent('Metal');
    expect(r.getByText('Built').nextSibling).toHaveTextContent(/^2026-10-25$/);
  });

  it("keeps one engine's install on its own row", async () => {
    fake.statuses = { llama: [status('llama', 'older')], stable_diffusion: [status('stable_diffusion', 'older')] };
    renderApp('/settings/engines');
    const llama = await row('llama.cpp');
    const sd = await row('stable-diffusion.cpp');
    await userEvent.click(await llama.findByRole('button', { name: 'Update to b11429…' }));
    await userEvent.click((await dialog()).getByRole('button', { name: 'Update' }));
    await waitFor(() => expect(fake.streams.llama).toHaveLength(1));
    fake.streams.llama[0].onEvent({ type: 'phase_started', phase: 'download' });

    expect(await llama.findByText('Downloading llama.cpp binaries…')).toBeInTheDocument();
    expect(sd.queryByText(/^Downloading/)).not.toBeInTheDocument();
    expect(sd.getByRole('button', { name: 'Update to master-948-228c707…' })).toBeEnabled();
    expect(fake.streams.stable_diffusion).toHaveLength(0);
  });

  it('reads the row back after an install that finished while Engines was closed', async () => {
    fake.statuses.llama = [status('llama', 'older'), status('llama', 'older'), status('llama', 'atPin')];
    const first = renderApp('/settings/engines');
    await userEvent.click(await (await row('llama.cpp')).findByRole('button', { name: 'Update to b11429…' }));
    await userEvent.click((await dialog()).getByRole('button', { name: 'Update' }));
    await waitFor(() => expect(fake.streams.llama).toHaveLength(1));
    first.unmount();

    renderApp('/settings/engines');
    const r = await row('llama.cpp');
    expect(await r.findByText('b10327 · macOS ARM64 (Metal) · prebuilt')).toBeInTheDocument();
    fake.streams.llama[0].onEvent({ type: 'completed', version: 'b11429' });

    expect(await r.findByText('b11429 · macOS ARM64 (Metal) · prebuilt')).toBeInTheDocument();
    expect(fake.streams.llama).toHaveLength(1);
  });

  it('says the CPU warning only where it offers an install', async () => {
    fake.statuses.llama = [status('llama', 'atPin', { warning: CPU_WARNING })];
    renderApp('/settings/engines');
    const r = await row('llama.cpp');
    expect(await r.findByText('Installed')).toBeInTheDocument();
    await openMore('llama.cpp');
    expect(r.getByRole('menuitem', { name: 'Reinstall' })).toBeInTheDocument();
    // Reinstall is offered, so the warning about what it downloads is said.
    expect(r.getByText(CPU_WARNING)).toBeInTheDocument();
  });

  it('says no CPU warning where nothing would be installed', async () => {
    fake.statuses.llama = [{ ...built('llama'), warning: CPU_WARNING }];
    renderApp('/settings/engines');
    expect(await (await row('llama.cpp')).findByText('Installed')).toBeInTheDocument();
    expect(screen.queryByText(CPU_WARNING)).not.toBeInTheDocument();
  });

  it('offers no Remove… while an install is running', async () => {
    renderApp('/settings/engines');
    let r = await openMore('llama.cpp');
    await userEvent.click(await r.findByRole('menuitem', { name: 'Reinstall' }));
    r = await openMore('llama.cpp');

    expect(r.getByRole('menuitem', { name: 'Remove…' })).toBeDisabled();
    expect(r.getByRole('menuitem', { name: 'Reinstall' })).toBeDisabled();
  });

  it('offers no Reinstall where no download fits, or where the install would build again', async () => {
    fake.statuses.llama = [status('llama', 'atPin', { download: null, downloadUnavailable: 'No download fits linux aarch64.' })];
    fake.statuses.stable_diffusion = [status('stable_diffusion', 'atPin', {}, { rebuilds: true })];
    renderApp('/settings/engines');

    let r = await openMore('llama.cpp');
    expect(r.queryByRole('menuitem', { name: 'Reinstall' })).not.toBeInTheDocument();
    expect(r.getByRole('menuitem', { name: 'Remove…' })).toBeInTheDocument();
    r = await openMore('stable-diffusion.cpp');
    expect(r.queryByRole('menuitem', { name: 'Reinstall' })).not.toBeInTheDocument();
    expect(r.getByRole('menuitem', { name: 'Remove…' })).toBeInTheDocument();
  });

  it('says why there is no install record: none, or the reason it could not be read', async () => {
    fake.statuses.llama = [status('llama', 'atPin', { install: null })];
    fake.statuses.stable_diffusion = [
      status('stable_diffusion', 'atPin', { install: null, recordError: 'sd-config.json: expected value at line 1' }),
    ];
    renderApp('/settings/engines');
    const llama = await row('llama.cpp');
    await userEvent.click(await llama.findByRole('button', { name: 'Details' }));
    expect(llama.getByText('Install record').nextSibling).toHaveTextContent(/^none, installed outside gglib$/);
    // With no record, the line has the release the pin compared and nothing it cannot know.
    expect(llama.getByText('b11429')).toBeInTheDocument();

    const sd = await row('stable-diffusion.cpp');
    await userEvent.click(await sd.findByRole('button', { name: 'Details' }));
    expect(sd.getByText('Install record').nextSibling).toHaveTextContent(/^sd-config\.json: expected value at line 1$/);
  });

  it("says a status that could not be read in the daemon's words", async () => {
    fake.statuses.llama = [new Error('the engine status could not be read: permission denied') as never];
    renderApp('/settings/engines');
    expect(await (await row('llama.cpp')).findByText('the engine status could not be read: permission denied')).toBeInTheDocument();
  });

  it("says a refused removal in the daemon's words", async () => {
    fake.refuseUninstall = 'llama.cpp is serving qwen; stop it first';
    renderApp('/settings/engines');
    const r = await openMore('llama.cpp');
    await userEvent.click(r.getByRole('menuitem', { name: 'Remove…' }));
    await userEvent.click((await dialog()).getByRole('button', { name: 'Remove' }));

    expect(await r.findByText('llama.cpp is serving qwen; stop it first')).toBeInTheDocument();
    expect(fake.uninstalled).toEqual([]);
  });

  it('keeps the system checks at the foot', async () => {
    renderApp('/settings/engines');
    expect(await screen.findByText('system checks')).toBeInTheDocument();
  });
});

describe('Faster downloads', () => {
  it('sets it up from Off, and reads On back', async () => {
    renderApp('/settings/engines');
    const r = await row('Faster downloads');
    expect(await r.findByText('Off')).toBeInTheDocument();
    expect(r.queryByText('gglib config fast-downloads enable')).not.toBeInTheDocument();

    await userEvent.click(r.getByRole('button', { name: 'Set up' }));

    expect(await r.findByText('On')).toBeInTheDocument();
    expect(fake.enabled).toBe(1);
    expect(r.queryByRole('button', { name: 'Set up' })).not.toBeInTheDocument();
  });

  it('turns it off from ⋯, and reads Off back', async () => {
    fake.setup.fastDownloadReady = true;
    renderApp('/settings/engines');
    const r = await row('Faster downloads');
    expect(await r.findByText('On')).toBeInTheDocument();

    await userEvent.click(r.getByRole('button', { name: 'More for Faster downloads' }));
    await userEvent.click(r.getByRole('menuitem', { name: 'Turn off' }));

    expect(await r.findByText('Off')).toBeInTheDocument();
    expect(fake.disabled).toBe(1);
    expect(r.getByRole('button', { name: 'Set up' })).toBeInTheDocument();
  });

  it('says a failed set up in the daemon\'s words, and stays Off', async () => {
    fake.refuseEnable = 'No Python 3.10+ interpreter was found';
    renderApp('/settings/engines');
    const r = await row('Faster downloads');
    await userEvent.click(await r.findByRole('button', { name: 'Set up' }));

    expect(await r.findByRole('alert')).toHaveTextContent('No Python 3.10+ interpreter was found');
    expect(r.getByText('Off')).toBeInTheDocument();
  });

  it('says it is setting up while it does, and offers no Turn off while one is on its way', async () => {
    let release = () => {};
    fake.hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    renderApp('/settings/engines');
    const r = await row('Faster downloads');
    await userEvent.click(await r.findByRole('button', { name: 'Set up' }));

    expect(await r.findByText('Setting up. This can take a minute.')).toBeInTheDocument();
    expect(r.getByRole('button', { name: 'Set up' })).toBeDisabled();
    release();
    expect(await r.findByText('On')).toBeInTheDocument();
    expect(r.queryByText('Setting up. This can take a minute.')).not.toBeInTheDocument();

    fake.hold = new Promise<void>(() => {});
    await userEvent.click(r.getByRole('button', { name: 'More for Faster downloads' }));
    await userEvent.click(r.getByRole('menuitem', { name: 'Turn off' }));
    await userEvent.click(r.getByRole('button', { name: 'More for Faster downloads' }));
    expect(r.getByRole('menuitem', { name: 'Turn off' })).toBeDisabled();
    expect(fake.disabled).toBe(1);
  });

  it("says a status that could not be read in the daemon's words", async () => {
    fake.refuseSetupStatus = 'the setup status could not be read';
    renderApp('/settings/engines');
    expect(await (await row('Faster downloads')).findByRole('alert')).toHaveTextContent(/^the setup status could not be read$/);
  });

  it('names the command, with a copy, where there is no Python, and offers no Set up', async () => {
    fake.setup.pythonAvailable = false;
    renderApp('/settings/engines');
    const r = await row('Faster downloads');

    expect(await r.findByText('gglib config fast-downloads enable')).toBeInTheDocument();
    expect(r.getByText('It needs Python 3.10 or newer. Once that is installed:')).toBeInTheDocument();
    expect(r.getByRole('button', { name: 'Copy the command' })).toBeInTheDocument();
    expect(r.queryByRole('button', { name: 'Set up' })).not.toBeInTheDocument();
  });
});
