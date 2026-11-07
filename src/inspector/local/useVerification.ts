/**
 * A model's file check and update check, held with the model rather than
 * the inspector: each model's status lives in one map here, so leaving a
 * model while its files are hashed and coming back still shows the check
 * (and its answer, when the daemon's reply lands while away), and a status
 * for one model is never read for another.
 *
 * The `verification` stream says how far a check is, for any model; it
 * moves a model that is idle or being checked, never one holding an answer,
 * so an event that arrives after the reply it belongs to cannot overwrite
 * it. A reply applies only when no later run on that model overtook it.
 * Each repair and re-download asks first, in the words the verification
 * dialog used, and then runs on the download queue.
 *
 * @module inspector/local/useVerification
 */

import { useCallback, useEffect, useState, useSyncExternalStore } from 'react';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { getTransport, type UpdateCheckResult, type VerificationReport } from '../../services/transport';
import type { VerificationEvent } from '../../services/transport/types/events';
import type { UpgradeCheck } from '../../types';
import { formatError } from '../../utils/errors';

export type VerifyStatus =
  | { kind: 'idle' }
  /** Files being hashed; `file` is the one now, named as the daemon names it, once the stream says. */
  | { kind: 'verifying'; file: { label: string; percent: number } | null }
  | { kind: 'verified'; report: VerificationReport }
  | { kind: 'checking' }
  | { kind: 'checked'; upgrade: UpgradeCheck; updates: UpdateCheckResult }
  /** A repair or re-download handed to the download queue. */
  | { kind: 'queued' }
  /** The daemon's refusal, and which check Retry asks again. */
  | { kind: 'failed'; message: string; retry: 'verify' | 'update' };

const IDLE: VerifyStatus = { kind: 'idle' };
const statuses = new Map<number, VerifyStatus>();
/** Each model's latest run: a reply from an earlier one is dropped. */
const runs = new Map<number, number>();
/** Models whose file check this window asked for and has not had answered. */
const asking = new Set<number>();
const listeners = new Set<() => void>();

function put(id: number, status: VerifyStatus): void {
  statuses.set(id, status);
  for (const listener of listeners) listener();
}

function listen(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Start a run on model `id` and return a setter that applies only while it is the latest. */
function begin(id: number, status: VerifyStatus): (next: VerifyStatus) => void {
  const run = (runs.get(id) ?? 0) + 1;
  runs.set(id, run);
  put(id, status);
  return (next) => {
    if (runs.get(id) === run) put(id, next);
  };
}

function onEvent(event: VerificationEvent): void {
  const now = statuses.get(event.modelId) ?? IDLE;
  if (now.kind !== 'idle' && now.kind !== 'verifying') return;
  if (event.type === 'verification_progress') {
    const percent = event.totalBytes > 0 ? Math.round((event.bytesProcessed / event.totalBytes) * 100) : 0;
    put(event.modelId, { kind: 'verifying', file: { label: event.shardName, percent } });
  } else if (event.type === 'verification_complete' && !asking.has(event.modelId)) {
    // A check this window did not ask for (the CLI's) has ended; its report went to whoever asked.
    put(event.modelId, IDLE);
  }
}

/** Forget every status (tests). */
export function resetVerifications(): void {
  statuses.clear();
  runs.clear();
  asking.clear();
}

async function runVerify(id: number): Promise<void> {
  // One check of a model's files at a time: the daemon refuses a second.
  if (asking.has(id)) return;
  const settle = begin(id, { kind: 'verifying', file: null });
  asking.add(id);
  try {
    settle({ kind: 'verified', report: await getTransport().verifyModel(id) });
  } catch (err) {
    settle({ kind: 'failed', message: formatError(err), retry: 'verify' });
  } finally {
    asking.delete(id);
  }
}

async function runCheck(id: number): Promise<void> {
  if (statuses.get(id)?.kind === 'checking') return;
  const settle = begin(id, { kind: 'checking' });
  try {
    const [upgrade, updates] = await Promise.all([getTransport().checkModelUpgrade(id), getTransport().checkModelUpdates(id)]);
    settle({ kind: 'checked', upgrade, updates });
  } catch (err) {
    settle({ kind: 'failed', message: formatError(err), retry: 'update' });
  }
}

/** The files a report calls damaged: corrupt or missing (a file with no hash is neither). */
export function damagedFiles(report: VerificationReport) {
  return report.shards.filter((s) => s.health.type === 'corrupt' || s.health.type === 'missing');
}

export interface Verification {
  status: VerifyStatus;
  /** A repair or re-download being asked or queued. */
  busy: boolean;
  verify: () => void;
  checkUpdates: () => void;
  /** Ask, then re-download the damaged files the report names. */
  repairDamaged: (report: VerificationReport) => Promise<void>;
  /** Ask, then download the files the update check says changed. */
  downloadChanged: (updates: UpdateCheckResult) => Promise<void>;
  /** Ask, then re-download the whole model at the latest revision. */
  redownloadLatest: () => Promise<void>;
}

export function useVerification(modelId: number, modelName: string): Verification {
  const { confirm } = useConfirmContext();
  const status = useSyncExternalStore(listen, () => statuses.get(modelId) ?? IDLE);
  const [busyFor, setBusyFor] = useState<number | null>(null);

  useEffect(() => getTransport().subscribe('verification', onEvent), []);

  const verify = useCallback(() => void runVerify(modelId), [modelId]);
  const checkUpdates = useCallback(() => void runCheck(modelId), [modelId]);

  /** Hand `start` to the queue once asked; its refusal is the line's, with Retry asking `retry` again. */
  const queue = async (asked: boolean, retry: 'verify' | 'update', start: () => Promise<boolean>) => {
    if (!asked) return;
    setBusyFor(modelId);
    try {
      if (await start()) put(modelId, { kind: 'queued' });
    } catch (err) {
      put(modelId, { kind: 'failed', message: formatError(err), retry });
    } finally {
      setBusyFor(null);
    }
  };

  const repairDamaged = async (report: VerificationReport) => {
    const damaged = damagedFiles(report);
    const asked = await confirm({
      title: 'Re-download unhealthy files?',
      description: `This will re-download ${damaged.length} unhealthy file(s) for "${modelName}".`,
      confirmLabel: 'Re-download',
    });
    await queue(asked, 'verify', async () => {
      await getTransport().repairModel(modelId, damaged.map((s) => s.index));
      return true;
    });
  };

  const downloadChanged = async (updates: UpdateCheckResult) => {
    const changes = updates.details?.changes ?? [];
    const asked = await confirm({
      title: 'Download updates?',
      description: `This will download ${updates.details?.changed_shards ?? changes.length} updated shard(s) for "${modelName}".`,
      confirmLabel: 'Download',
    });
    await queue(asked, 'update', async () => {
      await getTransport().repairModel(modelId, changes.map((c) => c.index));
      return true;
    });
  };

  const redownloadLatest = async () => {
    const asked = await confirm({
      title: 'Re-download at the latest revision?',
      description:
        `This downloads the whole of "${modelName}" again on the download queue, where its ` +
        'progress shows and it can be cancelled. Each file is replaced once its new copy is in, ' +
        'so cancelling part-way can leave a model with files of two revisions.',
      confirmLabel: 'Re-download',
      variant: 'danger',
    });
    await queue(asked, 'update', async () => {
      const queued = await getTransport().upgradeModel(modelId);
      // Nothing queued: already at the latest revision. Ask again, for the daemon's own answer.
      if (queued.id === null) void runCheck(modelId);
      return queued.id !== null;
    });
  };

  return { status, busy: busyFor === modelId, verify, checkUpdates, repairDamaged, downloadChanged, redownloadLatest };
}
