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
import { verifyModel, checkModelUpdates, repairModel, type VerificationReport, type UpdateCheckResult, type OverallHealth } from '../services/clients/verification';
import { getTransport } from '../services/transport';
import type { VerificationEvent } from '../services/transport/types/events';
import { appLogger } from '../services/platform';

interface VerificationModalProps {
  modelId: number;
  modelName: string;
  open: boolean;
  onClose: () => void;
  mode: 'verify' | 'update';
}

export const VerificationModal: FC<VerificationModalProps> = ({ modelId, modelName, open, onClose, mode }) => {
  const [verifying, setVerifying] = useState(false);
  const [progress, setProgress] = useState<{ shardName: string; percent: number } | null>(null);
  const [report, setReport] = useState<VerificationReport | null>(null);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  const handleVerify = async () => {
    setVerifying(true);
    setError(null);
    setReport(null);
    setProgress(null);

    try {
      const result = await verifyModel(modelId);
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
      const result = await checkModelUpdates(modelId);
      setUpdateResult(result);
      appLogger.info('component', 'Update check complete', { modelId, updateAvailable: result.update_available });
    } catch (err) {
      appLogger.error('component', 'Update check failed', { error: err, modelId });
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCheckingUpdates(false);
    }
  };

  const handleRepair = async () => {
    if (!report && !updateResult) return;

    const shardCount = report
      ? report.shards.filter(s => s.health.type === 'corrupt' || s.health.type === 'missing').length
      : updateResult?.details?.changed_shards ?? 0;

    const confirmed = window.confirm(
      mode === 'update'
        ? `This will download ${shardCount} updated shard(s) for "${modelName}". Continue?`
        : `This will re-download ${shardCount} corrupt shard(s) for "${modelName}". Continue?`
    );
    if (!confirmed) return;

    setRepairing(true);
    setError(null);

    try {
      // For updates, only repair changed shards if specified
      const shardsToRepair = updateResult?.details?.changes.map(c => c.index);
      await repairModel(modelId, shardsToRepair);
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

  const modalTitle = mode === 'verify' ? `Verify: ${modelName}` : `Check Updates: ${modelName}`;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={modalTitle}
      size="md"
      preventClose={verifying || repairing || checkingUpdates}
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
                  <span className="font-semibold text-primary">Updates Available</span>
                </>
              ) : (
                <>
                  <Icon icon={CheckCircle2} size={20} className="text-green-400" />
                  <span className="font-semibold text-primary">Model is Up to Date</span>
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

        {/* Initial Action Buttons */}
        {!report && !verifying && mode === 'verify' && (
          <Button onClick={handleVerify} className="w-full" disabled={verifying}>
            Start Verification
          </Button>
        )}

        {!updateResult && !checkingUpdates && mode === 'update' && (
          <Button onClick={handleCheckUpdates} className="w-full" disabled={checkingUpdates}>
            <Icon icon={RefreshCw} size={16} />
            Check for Updates
          </Button>
        )}
      </div>
    </Modal>
  );
};
