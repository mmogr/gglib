/**
 * Model verification modal with progress tracking.
 * Shows real-time SHA256 hashing progress via SSE.
 */

import { FC, useState, useEffect } from 'react';
import { CheckCircle2, XCircle, AlertCircle, Loader2, Wrench } from 'lucide-react';
import { Modal } from './ui/Modal';
import { Icon } from './ui/Icon';
import { Button } from './ui/Button';
import { verifyModel, repairModel, type VerificationReport, type OverallHealth } from '../services/clients/verification';
import { getTransport } from '../services/transport';
import type { VerificationEvent } from '../services/transport/types/events';
import { appLogger } from '../services/platform';

interface VerificationModalProps {
  modelId: number;
  modelName: string;
  open: boolean;
  onClose: () => void;
}

export const VerificationModal: FC<VerificationModalProps> = ({ modelId, modelName, open, onClose }) => {
  const [verifying, setVerifying] = useState(false);
  const [progress, setProgress] = useState<{ shardName: string; percent: number } | null>(null);
  const [report, setReport] = useState<VerificationReport | null>(null);
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

  const handleRepair = async () => {
    if (!report) return;

    const confirmed = window.confirm(
      `This will re-download corrupt shards for "${modelName}". Continue?`
    );
    if (!confirmed) return;

    setRepairing(true);
    setError(null);

    try {
      await repairModel(modelId);
     appLogger.info('component', 'Repair initiated', { modelId });
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

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={`Verify: ${modelName}`}
      size="md"
      preventClose={verifying || repairing}
    >
      <div className="flex flex-col gap-4">
        {error && (
          <div className="p-3 bg-red-50 border border-red-200 rounded text-red-700 text-sm">
            {error}
          </div>
        )}

        {verifying && progress && (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-2">
              <Loader2 className="animate-spin" size={16} />
              <span className="text-sm font-medium">Verifying {progress.shardName}...</span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-2">
              <div
                className="bg-blue-500 h-2 rounded-full transition-all duration-300"
                style={{ width: `${progress.percent}%` }}
              />
            </div>
            <span className="text-xs text-gray-500">{progress.percent}% complete</span>
          </div>
        )}

        {report && (
          <div className="flex flex-col gap-3">
            <div className="flex items-center gap-2 p-3 bg-gray-50 rounded">
              {getHealthIcon(report.overall_health)}
              <span className="font-medium">Overall Health: {getHealthLabel(report.overall_health)}</span>
            </div>

            <div className="text-sm">
              <div className="font-medium mb-2">Shards: {report.shards.length}</div>
              <div className="max-h-48 overflow-y-auto space-y-1">
                {report.shards.map((shard, idx) => (
                  <div key={idx} className="flex items-center gap-2 text-xs">
                    {shard.health.type === 'healthy' && <CheckCircle2 size={14} className="text-green-500" />}
                    {shard.health.type === 'corrupt' && <XCircle size={14} className="text-red-500" />}
                    {shard.health.type === 'missing' && <AlertCircle size={14} className="text-orange-500" />}
                    {shard.health.type === 'no_oid' && <AlertCircle size={14} className="text-yellow-500" />}
                    <span className="truncate">{shard.file_path.split('/').pop()}</span>
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

        {!report && !verifying && (
          <Button onClick={handleVerify} className="w-full" disabled={verifying}>
            Start Verification
          </Button>
        )}
      </div>
    </Modal>
  );
};
