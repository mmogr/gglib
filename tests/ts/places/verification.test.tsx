/**
 * Verify files and Check for updates, from the inspector's ⋯: each opens
 * Details and writes its line under Source. The file check's progress comes
 * from the `verification` stream; its answer is intact, or damaged with a
 * re-download that asks first and names those files, and opens a list of
 * every file. The update check composes the revision check and the file
 * check: up to date, or a newer revision with Download changed files and
 * Re-download at latest…, each asking first. A refusal is the daemon's
 * words with Retry. A status is the model's own: it survives leaving the
 * model and never shows on another.
 *
 * The transport is a fake whose verification calls are the real API
 * functions over a fake `get` and `post`, so what is checked is the path
 * and the body the daemon would receive.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, cleanup, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

const library = vi.hoisted(() => ({ models: [] as unknown[] }));
const stream = vi.hoisted(() => ({ handler: null as ((e: unknown) => void) | null }));
const daemon = vi.hoisted(() => ({
  verify: null as null | (() => Promise<unknown>),
  upgrade: null as unknown,
  updates: null as unknown,
  queued: { latestSha: 'b'.repeat(40), id: 'dl-2' } as { latestSha: string; id: string | null },
}));
const post = vi.hoisted(() => vi.fn(async (_path: string, _body?: unknown): Promise<unknown> => null));
const get = vi.hoisted(() => vi.fn(async (_path: string): Promise<unknown> => null));

vi.mock('../../../src/services/transport', async () => {
  const actual = await vi.importActual<Record<string, unknown>>('../../../src/services/transport');
  const verification = await import('../../../src/services/transport/api/verification');
  const local = await import('../../../src/services/transport/api/models/local');
  return {
    ...actual,
    getTransport: () => ({
      listModels: vi.fn(async () => library.models),
      listServers: vi.fn(async () => []),
      getDownloadQueue: vi.fn(async () => null),
      listPairedModels: vi.fn(async () => null),
      getModelDetail: vi.fn(async () => null),
      getSettings: vi.fn(async () => ({})),
      onEventStreamOpen: vi.fn(() => () => {}),
      revealSecret: vi.fn(async () => null),
      subscribe: vi.fn((category: string, handler: (e: unknown) => void) => {
        if (category !== 'verification') return () => {};
        stream.handler = handler;
        return () => {
          if (stream.handler === handler) stream.handler = null;
        };
      }),
      verifyModel: verification.verifyModel,
      checkModelUpdates: verification.checkModelUpdates,
      repairModel: verification.repairModel,
      checkModelUpgrade: local.checkModelUpgrade,
      upgradeModel: local.upgradeModel,
    }),
  };
});
vi.mock('../../../src/services/transport/api/client', () => ({
  get: (path: string) => get(path),
  post: (path: string, body?: unknown) => post(path, body),
}));
vi.mock('../../../src/services/transport/api/setup', () => ({
  getSetupStatus: vi.fn(async () => ({ llamaInstalled: true, llamaCanDownload: true })),
  getEngineStatus: vi.fn(async () => ({ installed: true })),
  streamEngineInstall: vi.fn(() => () => {}),
  getRecommendedModel: vi.fn(async () => null),
}));
vi.mock('../../../src/services/remoteEvents', () => ({ refreshRemoteStatus: vi.fn() }));

import { cleanupModelEvents, initModelEvents } from '../../../src/services/modelEvents';
import { resetVerifications } from '../../../src/inspector/local/useVerification';
import { TransportError } from '../../../src/services/transport/errors';
import type { VerificationReport } from '../../../src/services/transport';
import type { UpgradeCheck } from '../../../src/types';
import { renderApp } from './harness';
import { guiModel } from '../fixtures/model';

const GEMMA = guiModel({ id: 9, name: 'Gemma-3-4B-It', hfRepoId: 'unsloth/gemma-3-4b-it-GGUF' });
const QWEN = guiModel({ id: 10, name: 'Qwen3-8B', hfRepoId: 'Qwen/Qwen3-8B-GGUF' });
const LOCAL = guiModel({ id: 11, name: 'Home-Made', hfRepoId: null });

const OLD = { kind: 'sha' as const, value: 'a'.repeat(40), short: 'aaaaaaa' };
const NEW = { kind: 'sha' as const, value: 'b'.repeat(40), short: 'bbbbbbb' };

const shard = (index: number, health: VerificationReport['shards'][number]['health']) => ({
  index,
  kind: { kind: 'shard' as const, number: index + 1, of: 3 },
  file_path: `unsloth/gemma/gemma-0000${index + 1}-of-00003.gguf`,
  health,
});

function report(shards: VerificationReport['shards'], overall: VerificationReport['overall_health']): VerificationReport {
  return { model_id: 9, overall_health: overall, shards, verified_at: new Date().toISOString() };
}

/** A file check the test answers when it chooses. */
function pendingVerify() {
  let answer: (r: VerificationReport) => void = () => {};
  let refuse: (e: unknown) => void = () => {};
  daemon.verify = () =>
    new Promise((resolve, reject) => {
      answer = (r) => resolve({ report: r });
      refuse = reject;
    });
  return { answer: (r: VerificationReport) => act(() => answer(r)), refuse: (e: unknown) => act(() => refuse(e)) };
}

function emit(event: Record<string, unknown>) {
  act(() => stream.handler?.(event));
}

const progress = (modelId: number, shardName: string, bytesProcessed: number, totalBytes = 100) => ({
  type: 'verification_progress',
  modelId,
  modelName: 'x',
  shardName,
  bytesProcessed,
  totalBytes,
});

function renderInspector(model = GEMMA) {
  library.models = [GEMMA, QWEN, LOCAL];
  initModelEvents();
  return renderApp(`/models/${model.id}`);
}

async function pick(item: string) {
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'More' }));
  await user.click(screen.getByRole('menuitem', { name: item }));
  return user;
}

const details = () => screen.getByText('Details').closest('details')!;
const posts = (suffix: string) => post.mock.calls.filter(([path]) => path.endsWith(suffix));

describe('verify and update results, in Details › Source', () => {
  beforeEach(() => {
    resetVerifications();
    stream.handler = null;
    post.mockReset();
    get.mockReset();
    daemon.upgrade = { hasUpdate: false, current: OLD, latestSha: OLD.value, latest: OLD } satisfies UpgradeCheck;
    daemon.updates = { model_id: 9, update_available: false, details: null };
    daemon.queued = { latestSha: NEW.value, id: 'dl-2' };
    post.mockImplementation(async (path: string) => {
      if (path.endsWith('/verify')) return daemon.verify!();
      if (path.endsWith('/repair')) return { id: 'dl-1', files: [] };
      if (path.endsWith('/upgrade')) return daemon.queued;
      return null;
    });
    get.mockImplementation(async (path: string) => {
      if (path.endsWith('/upgrade-check')) return daemon.upgrade;
      if (path.endsWith('/updates')) return { result: daemon.updates, message: '' };
      return null;
    });
    try {
      localStorage.clear();
    } catch {
      // no storage in this environment
    }
  });
  afterEach(() => {
    cleanup();
    cleanupModelEvents();
  });

  it('opens Details and says how far the check is, from the stream', async () => {
    const run = pendingVerify();
    renderInspector();
    await pick('Verify files');

    await waitFor(() => expect(details()).toHaveAttribute('open'));
    expect(posts('/models/9/verify')).toHaveLength(1);
    expect(screen.getByText('Checking files…')).toBeInTheDocument();

    emit(progress(9, 'Shard 2/3', 45));
    expect(screen.getByText('Checking Shard 2/3… 45%')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Files checked' })).toHaveAttribute('aria-valuenow', '45');

    run.answer(report([shard(0, { type: 'healthy' })], 'healthy'));
    expect(await screen.findByText('Files intact')).toBeInTheDocument();
    // A late event from the run just answered does not undo the answer.
    emit(progress(9, 'Shard 3/3', 99));
    expect(screen.getByText('Files intact')).toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
  });

  it('names damaged files, lists every file on asking, and re-downloads those after asking', async () => {
    const run = pendingVerify();
    renderInspector();
    const user = await pick('Verify files');
    const expected = 'e'.repeat(64);
    const actual = 'f'.repeat(64);
    run.answer(
      report([shard(0, { type: 'healthy' }), shard(1, { type: 'corrupt', expected, actual }), shard(2, { type: 'missing' })], 'unhealthy'),
    );

    expect(await screen.findByText('2 files damaged')).toBeInTheDocument();
    // The list is closed until opened from the line.
    const list = screen.getByRole('list', { name: 'Files checked', hidden: true });
    expect(list.closest('details')).not.toHaveAttribute('open');
    await user.click(screen.getByText('3 files'));
    expect(list.closest('details')).toHaveAttribute('open');
    const rows = within(list).getAllByRole('listitem');
    expect(rows.map((r) => r.textContent)).toEqual([
      'Shard 1/3gemma-00001-of-00003.ggufHealthy',
      'Shard 2/3gemma-00002-of-00003.ggufCorrupt',
      'Shard 3/3gemma-00003-of-00003.ggufMissing',
    ]);
    expect(rows[1]).toHaveAttribute('title', `Expected ${expected}\nActual ${actual}`);

    // Cancelled, nothing is sent.
    await user.click(screen.getByRole('button', { name: 'Re-download them' }));
    const ask = await screen.findByRole('dialog', { name: 'Re-download unhealthy files?' });
    expect(ask).toHaveTextContent('This will re-download 2 unhealthy file(s) for "Gemma-3-4B-It".');
    await user.click(within(ask).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(posts('/repair')).toHaveLength(0);

    await user.click(screen.getByRole('button', { name: 'Re-download them' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Re-download' }));
    await waitFor(() => expect(posts('/models/9/repair')).toEqual([['/api/models/9/repair', { shards: [1, 2] }]]));
    expect(await screen.findByText(/Downloading on the queue/)).toBeInTheDocument();
  });

  it('counts a missing file as damaged and one with no hash as not, and one file is "it"', async () => {
    const run = pendingVerify();
    renderInspector();
    await pick('Verify files');
    run.answer(report([shard(0, { type: 'missing' }), shard(1, { type: 'no_oid' })], 'unhealthy'));
    expect(await screen.findByText('1 file damaged')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Re-download it' })).toBeInTheDocument();
  });

  it("says the daemon's refusal and asks again on Retry", async () => {
    const run = pendingVerify();
    renderInspector();
    const user = await pick('Verify files');
    run.refuse(new TransportError('INTERNAL', 'Failed to start verification: Model 9 is already being downloaded'));

    expect(await screen.findByRole('alert')).toHaveTextContent('Failed to start verification: Model 9 is already being downloaded');
    pendingVerify();
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(posts('/models/9/verify')).toHaveLength(2);
    expect(await screen.findByText('Checking files…')).toBeInTheDocument();
  });

  it('keeps a check with its model: back on it, its answer shows; on another model, nothing', async () => {
    const run = pendingVerify();
    const { router } = renderInspector();
    await pick('Verify files');
    emit(progress(9, 'Shard 1/3', 10));

    await act(() => router.navigate('/models/10'));
    expect(await screen.findByRole('heading', { name: 'Qwen3-8B' })).toBeInTheDocument();
    // Model 9's progress is not model 10's.
    emit(progress(9, 'Shard 2/3', 50));
    expect(screen.queryByText(/Checking/)).not.toBeInTheDocument();

    // Back on model 9 while its files are hashed: the check, as far as it is.
    await act(() => router.navigate('/models/9'));
    expect(await screen.findByText('Checking Shard 2/3… 50%')).toBeInTheDocument();

    // Answered while away on model 10: the answer is model 9's alone.
    await act(() => router.navigate('/models/10'));
    await screen.findByRole('heading', { name: 'Qwen3-8B' });
    run.answer(report([shard(0, { type: 'healthy' })], 'healthy'));
    expect(screen.queryByText('Files intact')).not.toBeInTheDocument();
    await act(() => router.navigate('/models/9'));
    expect(await screen.findByText('Files intact')).toBeInTheDocument();
  });

  it('drops an answer that a later check on the model overtook', async () => {
    let answerUpgrade: (v: unknown) => void = () => {};
    get.mockImplementation(async (path: string) => {
      if (path.endsWith('/upgrade-check')) return new Promise((resolve) => (answerUpgrade = resolve));
      if (path.endsWith('/updates')) return { result: daemon.updates, message: '' };
      return null;
    });
    pendingVerify();
    renderInspector();
    await pick('Check for updates');
    expect(await screen.findByText('Checking for updates…')).toBeInTheDocument();
    await pick('Verify files');
    expect(await screen.findByText('Checking files…')).toBeInTheDocument();

    await act(async () => answerUpgrade(daemon.upgrade));
    expect(screen.getByText('Checking files…')).toBeInTheDocument();
    expect(screen.queryByText(/Up to date/)).not.toBeInTheDocument();
  });

  it("shows a check the daemon reports for this model, though another client asked, until it ends", async () => {
    renderInspector();
    await screen.findByRole('heading', { name: 'Gemma-3-4B-It' });
    await waitFor(() => expect(stream.handler).not.toBeNull());
    emit(progress(9, 'Projector', 30));
    expect(screen.getByText('Checking Projector… 30%')).toBeInTheDocument();
    emit({ type: 'verification_complete', modelId: 9, modelName: 'x', overallHealth: 'healthy' });
    expect(screen.queryByText(/Checking/)).not.toBeInTheDocument();
  });

  it('says up to date when neither the revision nor a file changed', async () => {
    renderInspector();
    await pick('Check for updates');
    await waitFor(() => expect(details()).toHaveAttribute('open'));
    expect(await screen.findByText('Up to date (aaaaaaa)')).toBeInTheDocument();
    expect(get).toHaveBeenCalledWith('/api/models/9/upgrade-check');
    expect(get).toHaveBeenCalledWith('/api/models/9/updates');
    expect(screen.queryByRole('button', { name: /Download|Re-download/ })).not.toBeInTheDocument();
  });

  it('says a newer revision may be README only, and re-downloads at the latest after asking', async () => {
    daemon.upgrade = { hasUpdate: true, current: OLD, latestSha: NEW.value, latest: NEW } satisfies UpgradeCheck;
    renderInspector();
    const user = await pick('Check for updates');

    expect(await screen.findByText('A newer revision exists (may be README only)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Download changed files' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Re-download at latest…' }));
    const ask = await screen.findByRole('dialog', { name: 'Re-download at the latest revision?' });
    expect(ask).toHaveTextContent('This downloads the whole of "Gemma-3-4B-It" again on the download queue');
    await user.click(within(ask).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(posts('/upgrade')).toHaveLength(0);

    await user.click(screen.getByRole('button', { name: 'Re-download at latest…' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Re-download' }));
    await waitFor(() => expect(posts('/models/9/upgrade')).toHaveLength(1));
    expect(await screen.findByText(/Downloading on the queue/)).toBeInTheDocument();
  });

  it('downloads the changed files after asking, naming them by index', async () => {
    daemon.upgrade = { hasUpdate: true, current: OLD, latestSha: NEW.value, latest: NEW } satisfies UpgradeCheck;
    daemon.updates = {
      model_id: 9,
      update_available: true,
      details: {
        changed_shards: 2,
        changes: [
          { index: 0, file_path: 'a.gguf', old_oid: 'o', new_oid: 'n' },
          { index: 2, file_path: 'c.gguf', old_oid: 'o', new_oid: 'n' },
        ],
      },
    };
    renderInspector();
    const user = await pick('Check for updates');
    expect(await screen.findByText('A newer revision exists: 2 files changed')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Download changed files' }));
    const ask = await screen.findByRole('dialog', { name: 'Download updates?' });
    expect(ask).toHaveTextContent('This will download 2 updated shard(s) for "Gemma-3-4B-It".');
    await user.click(within(ask).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(posts('/repair')).toHaveLength(0);

    await user.click(screen.getByRole('button', { name: 'Download changed files' }));
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Download' }));
    await waitFor(() => expect(posts('/models/9/repair')).toEqual([['/api/models/9/repair', { shards: [0, 2] }]]));
  });

  it('asks again after a re-download that queued nothing, for the daemon to say it is current', async () => {
    daemon.upgrade = { hasUpdate: true, current: OLD, latestSha: NEW.value, latest: NEW } satisfies UpgradeCheck;
    daemon.queued = { latestSha: NEW.value, id: null };
    renderInspector();
    const user = await pick('Check for updates');
    await user.click(await screen.findByRole('button', { name: 'Re-download at latest…' }));
    daemon.upgrade = { hasUpdate: false, current: NEW, latestSha: NEW.value, latest: NEW } satisfies UpgradeCheck;
    await user.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'Re-download' }));
    expect(await screen.findByText('Up to date (bbbbbbb)')).toBeInTheDocument();
  });

  it('says so when no revision is recorded, still offering the re-download', async () => {
    daemon.upgrade = { hasUpdate: true, current: null, latestSha: NEW.value, latest: NEW } satisfies UpgradeCheck;
    renderInspector();
    await pick('Check for updates');
    expect(await screen.findByText('No revision is recorded for this model, so there is nothing to compare.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Re-download at latest…' })).toBeInTheDocument();
  });

  it("says an update check's refusal, and Retry checks again", async () => {
    get.mockImplementation(async (path: string) => {
      if (path.endsWith('/upgrade-check')) throw new TransportError('INTERNAL', 'Failed to reach HuggingFace');
      if (path.endsWith('/updates')) return { result: daemon.updates, message: '' };
      return null;
    });
    renderInspector();
    const user = await pick('Check for updates');
    expect(await screen.findByRole('alert')).toHaveTextContent('Failed to reach HuggingFace');

    get.mockImplementation(async (path: string) => {
      if (path.endsWith('/upgrade-check')) return daemon.upgrade;
      if (path.endsWith('/updates')) return { result: daemon.updates, message: '' };
      return null;
    });
    await user.click(screen.getByRole('button', { name: 'Retry' }));
    expect(await screen.findByText('Up to date (aaaaaaa)')).toBeInTheDocument();
  });

  it('offers Verify files but not Check for updates for a model from no repository', async () => {
    const run = pendingVerify();
    renderInspector(LOCAL);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'More' }));
    expect(screen.queryByRole('menuitem', { name: 'Check for updates' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('menuitem', { name: 'Verify files' }));
    expect(posts('/models/11/verify')).toHaveLength(1);
    // No repository, and still a Source line for the check to write.
    run.answer(report([shard(0, { type: 'no_oid' })], 'unverifiable'));
    expect(await screen.findByText('Unverifiable: no file has a hash to check against')).toBeInTheDocument();
  });
});
