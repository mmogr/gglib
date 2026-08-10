/**
 * Model verification modal with progress tracking.
 * Shows real-time SHA256 hashing progress via SSE.
 * Supports both verification and update checking modes.
 */

import { FC, useState, useEffect } from 'react';
import { CheckCircle2, XCircle, AlertCircle, Loader2, Wrench, RefreshCw } from 'lucide-react';
import { Modal } from './ui/Modal';
import { Icon } from './ui/Icon';
import { Button } from './ui/Button';
import { getTransport } from '../services/transport';
import type { VerificationEvent } from '../services/transport/types/events';
import { appLogger } from '../services/platform';
import { useConfirmContext } from '../contexts/ConfirmContext';
import type { VerificationReport, UpdateCheckResult, OverallHealth } from '../services/transport';
import type { UpgradeCheck } from '../types';

/** Commit SHAs are 40 hex chars; 8 is the conventional readable prefix. */
const shortSha = (sha: string) => sha.slice(0, 8);

interface VerificationModalProps {
  modelId: number;
  modelName: string;
  open: boolean;
  onClose: () => void;
  mode: 'verify' | 'update';
}

export const VerificationModal: FC<VerificationModalProps> = ({ modelId, modelName, open, onClose, mode }) => {
  const [verifying, setVerifying] = useState(false);
  const [upgrading, setUpgrading] = useState(false);
  const [upgradeMessage, setUpgradeMessage] = useState<string | null>(null);
  const [upgradeCheck, setUpgradeCheck] = useState<UpgradeCheck | null>(null);
  const [upgradeCheckFailed, setUpgradeCheckFailed] = useState(false);
  const [progress, setProgress] = useState<{ shardName: string; percent: number } | null>(null);
  const [report, setReport] = useState<VerificationReport | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { confirm } = useConfirmContext();

  useEffect(() => {
    if (!open) return;

    // Subscribe to verification events
    const unsubscribe = getTransport().subscribe('verification', (event: VerificationEvent) => {
      if (event.type === 'verification_progress' && event.modelId === modelId) {
        const percent = Math.round((event.bytesProcessed / event.totalBytes) * 100);
        setProgress({ shardName: event.shardName, percent });
      } else if (event.type === 'verification_complete' && event.modelId === modelId) {
        setVerifying(false);
        setProgress(null);
      }
    });

    return () => unsubscribe();
  }, [open, modelId]);

  // Answer "am I on the latest revision?" up front, so the upgrade below is a
  // decision rather than a gamble. Cheap (one HF metadata call) and it is the
  // only way to learn the answer without committing to a full re-download.
  useEffect(() => {
    if (!open || mode !== 'update') return;
    let cancelled = false;
    setUpgradeCheck(null);
    setUpgradeCheckFailed(false);
    void getTransport()
      .checkModelUpgrade(modelId)
      .then((check) => {
        if (!cancelled) setUpgradeCheck(check);
      })
      .catch((err) => {
        // Non-fatal: the section says so and still offers the re-download.
        appLogger.warn('component', 'Upgrade check failed', { error: err, modelId });
        if (!cancelled) setUpgradeCheckFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [open, mode, modelId]);

  const handleVerify = async () => {
    setVerifying(true);
    setError(null);
    setReport(null);
    setProgress(null);

    try {
      const result = await getTransport().verifyModel(modelId);
      setReport(result);
      appLogger.info('component', 'Verification complete', { modelId, health: result.overall_health });
    } catch (err) {
      appLogger.error('component', 'Verification failed', { error: err, modelId });
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setVerifying(false);
      setProgress(null);
    }
  };

  const handleCheckUpdates = async () => {
    setCheckingUpdates(true);
    setError(null);
    setUpdateResult(null);

    try {
      const result = await getTransport().checkModelUpdates(modelId);
      setUpdateResult(result);
      appLogger.info('component', 'Update check complete', { modelId, updateAvailable: result.update_available });
    } catch (err) {
      appLogger.error('component', 'Update check failed', { error: err, modelId });
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingUpdates(false);
    }
  };

  /**
   * SHA-level re-download (`gglib model upgrade`), distinct from the
   * shard-repair path above: that fetches the shards whose contents changed,
   * this replaces the whole model at a newer repository revision.
   *
   * Confirmed through the same dialog as repair, not a label swap — it
   * replaces a file that may be tens of gigabytes. The modal is sealed while
   * it runs; the download itself completes server-side regardless.
   */
  const handleUpgrade = async () => {
    const confirmed = await confirm({
      title: 'Re-download at the latest revision?',
      description:
        `This downloads the whole of "${modelName}" again and replaces the current file. ` +
        'There is no progress reporting yet, and it can take a long time on a large model — ' +
        'leave gglib running until it finishes.',
      confirmLabel: 'Re-download',
      variant: 'danger',
    });
    if (!confirmed) return;

    setUpgrading(true);
    setError(null);
    setUpgradeMessage(null);
    try {
      const outcome = await getTransport().upgradeModel(modelId);
      setUpgradeMessage(
        outcome.updated
          ? `Upgraded to revision ${shortSha(outcome.latestSha)}`
          : `Already at the latest revision (${shortSha(outcome.latestSha)})`,
      );
      setUpgradeCheck({
        hasUpdate: false,
        currentSha: outcome.latestSha,
        latestSha: outcome.latestSha,
      });
      appLogger.info('component', 'Model upgrade finished', { modelId, updated: outcome.updated });
    } catch (err) {
      appLogger.error('component', 'Model upgrade failed', { error: err, modelId });
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setUpgrading(false);
    }
  };

  const handleRepair = async () => {
    if (!report && !updateResult) return;

    const shardCount = report
      ? report.shards.filter(s => s.health.type === 'corrupt' || s.health.type === 'missing').length
      : updateResult?.details?.changed_shards ?? 0;

    const confirmed = await confirm({
      title: mode === 'update' ? 'Download updates?' : 'Re-download corrupt shards?',
      description: mode === 'update'
        ? `This will download ${shardCount} updated shard(s) for "${modelName}".`
        : `This will re-download ${shardCount} corrupt shard(s) for "${modelName}".`,
      confirmLabel: mode === 'update' ? 'Download' : 'Re-download',
    });
    if (!confirmed) return;

    setRepairing(true);
    setError(null);

    try {
      // For updates, only repair changed shards if specified
      const shardsToRepair = updateResult?.details?.changes.map(c => c.index);
      await getTransport().repairModel(modelId, shardsToRepair);
      appLogger.info('component', 'Repair initiated', { modelId, mode });
      // Close modal and let user monitor downloads
      onClose();
    } catch (err) {
      appLogger.error('component', 'Repair failed', { error: err, modelId });
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRepairing(false);
    }
  };

  const getHealthIcon = (health: OverallHealth) => {
    switch (health) {
      case 'healthy':
        return <Icon icon={CheckCircle2} size={20} className="text-green-500" />;
      case 'unhealthy':
        return <Icon icon={XCircle} size={20} className="text-red-500" />;
      case 'unverifiable':
        return <Icon icon={AlertCircle} size={20} className="text-yellow-500" />;
    }
  };

  const getHealthLabel = (health: OverallHealth) => {
    switch (health) {
      case 'healthy':
        return 'Healthy';
      case 'unhealthy':
        return 'Unhealthy';
      case 'unverifiable':
        return 'Unverifiable';
    }
  };

  const hasCorruptShards = report?.shards.some(
    (shard) => shard.health.type === 'corrupt' || shard.health.type === 'missing'
  ) ?? false;

  const hasUpdates = updateResult?.update_available ?? false;

  /**
   * The revision check in one sentence. A model imported without a recorded
   * revision reports `hasUpdate` only because there is no baseline to compare
   * against — calling that "an update is available" would be a guess, so it
   * says what is actually known instead.
   */
  const revisionSummary = upgradeCheckFailed
    ? 'Could not reach HuggingFace to compare revisions.'
    : !upgradeCheck
      ? 'Checking the repository revision…'
      : upgradeCheck.currentSha == null
        ? `No revision is recorded for this model, so there is nothing to compare. Re-downloading fetches ${shortSha(upgradeCheck.latestSha)} and records it.`
        : upgradeCheck.hasUpdate
          ? `A newer revision is available: ${shortSha(upgradeCheck.currentSha)} → ${shortSha(upgradeCheck.latestSha)}.`
          : `Already at the latest revision (${shortSha(upgradeCheck.currentSha)}).`;

  const modalTitle = mode === 'verify' ? `Verify: ${modelName}` : `Check Updates: ${modelName}`;

  const footerAction =
    !report && !verifying && mode === 'verify' ? (
      <Button onClick={handleVerify} className="w-full" disabled={verifying}>
        Start Verification
      </Button>
    ) : !updateResult && !checkingUpdates && mode === 'update' ? (
      <Button onClick={handleCheckUpdates} className="w-full" disabled={checkingUpdates}>
        <Icon icon={RefreshCw} size={16} />
        Check for Updates
      </Button>
    ) : null;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={modalTitle}
      size="md"
      preventClose={verifying || repairing || checkingUpdates || upgrading}
      footer={footerAction ?? undefined}
    >
      <div className="flex flex-col gap-4">
        {error && (
          <div className="p-3 bg-red-500/10 border border-red-500/50 rounded text-red-400 text-sm">
            {error}
          </div>
        )}

        {/* Verification Progress */}
        {verifying && (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <Loader2 className="animate-spin text-primary" size={16} />
              <span className="text-sm font-medium text-primary">
                {progress ? `Verifying ${progress.shardName}...` : 'Starting verification...'}
              </span>
            </div>
            {progress && (
              <>
                <div className="w-full bg-surface-elevated rounded-full h-2 border border-subtle">
                  <div
                    className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                    style={{ width: `${progress.percent}%` }}
                  />
                </div>
                <span className="text-xs text-secondary">{progress.percent}% complete</span>
              </>
            )}
          </div>
        )}

        {/* Checking Updates Loading */}
        {checkingUpdates && (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <Loader2 className="animate-spin text-primary" size={16} />
              <span className="text-sm font-medium text-primary">Checking HuggingFace for updates...</span>
            </div>
          </div>
        )}

        {/* Verification Report */}
        {report && mode === 'verify' && (
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-2 p-3 bg-surface-elevated border-2 border-strong rounded">
              {getHealthIcon(report.overall_health)}
              <span className="font-semibold text-primary">Overall Health: {getHealthLabel(report.overall_health)}</span>
            </div>

            <div className="text-sm">
              <div className="font-semibold text-primary mb-2">Shards: {report.shards.length}</div>
              <div className="max-h-48 overflow-y-auto space-y-2 border border-base rounded p-3 bg-surface">
                {report.shards.map((shard, idx) => (
                  <div key={idx} className="flex flex-col gap-1 p-2 bg-surface-elevated rounded border border-subtle">
                    <div className="flex items-center gap-2">
                      {shard.health.type === 'healthy' && <CheckCircle2 size={14} className="text-green-500 flex-shrink-0" />}
                      {shard.health.type === 'corrupt' && <XCircle size={14} className="text-red-500 flex-shrink-0" />}
                      {shard.health.type === 'missing' && <AlertCircle size={14} className="text-orange-500 flex-shrink-0" />}
                      {shard.health.type === 'no_oid' && <AlertCircle size={14} className="text-yellow-500 flex-shrink-0" />}
                      <span className="truncate text-primary font-medium">{shard.file_path.split('/').pop()}</span>
                    </div>
                    <div className="ml-6 text-xs text-secondary">
                      Status: <span className="font-mono">{shard.health.type}</span>
                      {shard.health.type === 'corrupt' && shard.health.expected && (
                        <div className="mt-1 text-red-400">Expected: {shard.health.expected.substring(0, 12)}...</div>
                      )}
                      {shard.health.type === 'corrupt' && shard.health.actual && (
                        <div className="text-red-400">Actual: {shard.health.actual.substring(0, 12)}...</div>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {hasCorruptShards && (
              <Button
                onClick={handleRepair}
                disabled={repairing}
                className="w-full"
              >
                {repairing ? (
                  <>
                    <Loader2 className="animate-spin" size={16} />
                    Repairing...
                  </>
                ) : (
                  <>
                    <Icon icon={Wrench} size={16} />
                    Repair Model
                  </>
                )}
              </Button>
            )}
          </div>
        )}

        {/* Update Check Result */}
        {updateResult && mode === 'update' && (
          <div className="flex flex-col gap-3">
            <div className={`flex items-center gap-2 p-3 rounded border-2 ${
              hasUpdates ? 'bg-blue-500/10 border-blue-500' : 'bg-green-500/10 border-green-500'
            }`}>
              {hasUpdates ? (
                <>
                  <Icon icon={RefreshCw} size={20} className="text-blue-400" />
                  <span className="font-semibold text-primary">Shard Updates Available</span>
                </>
              ) : (
                <>
                  <Icon icon={CheckCircle2} size={20} className="text-green-400" />
                  <span className="font-semibold text-primary">Shards Are Up to Date</span>
                </>
              )}
            </div>

            {hasUpdates && updateResult.details && (
              <div className="text-sm">
                <div className="font-semibold text-primary mb-2">
                  {updateResult.details.changed_shards} shard(s) have updates
                </div>
                <div className="max-h-48 overflow-y-auto space-y-2 border border-base rounded p-3 bg-surface">
                  {updateResult.details.changes.map((change, idx) => (
                    <div key={idx} className="flex flex-col gap-1 text-xs p-2 bg-surface-elevated rounded border border-subtle">
                      <div className="flex items-center gap-2">
                        <RefreshCw size={12} className="text-blue-400" />
                        <span className="truncate font-medium text-primary">{change.file_path.split('/').pop()}</span>
                      </div>
                      <div className="ml-5 text-secondary space-y-0.5 font-mono text-xs">
                        <div className="truncate">Old: {change.old_oid.substring(0, 12)}...</div>
                        <div className="truncate">New: {change.new_oid.substring(0, 12)}...</div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {hasUpdates && (
              <Button
                onClick={handleRepair}
                disabled={repairing}
                className="w-full"
              >
                {repairing ? (
                  <>
                    <Loader2 className="animate-spin" size={16} />
                    Downloading Updates...
                  </>
                ) : (
                  <>
                    <Icon icon={RefreshCw} size={16} />
                    Download Updates
                  </>
                )}
              </Button>
            )}
          </div>
        )}

        {/* Full-revision upgrade — the GUI face of `gglib model upgrade` */}
        {mode === 'update' && (
          <section className="flex flex-col gap-sm mt-xl">
            <h3 className="m-0 text-sm font-semibold text-text">Full re-download</h3>
            <p className="text-xs text-text-muted m-0">
              The check above compares shard contents. This compares the repository revision and
              replaces the entire model file with the newest one.
            </p>
            <p className="text-xs text-text-muted m-0">{revisionSummary}</p>
            <Button
              variant="secondary"
              isLoading={upgrading}
              disabled={upgrading || upgradeCheck?.hasUpdate === false}
              onClick={handleUpgrade}
              className="w-full"
            >
              {upgrading ? 'Re-downloading…' : 'Re-download at latest revision'}
            </Button>
            {upgradeMessage && (
              <p className="text-xs text-text-muted m-0" role="status">
                {upgradeMessage}
              </p>
            )}
          </section>
        )}

      </div>
    </Modal>
  );
};
