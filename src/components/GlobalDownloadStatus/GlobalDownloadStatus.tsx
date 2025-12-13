import { FC, useState, useEffect, useRef, useCallback } from 'react';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import type { DownloadProgressView } from '../../hooks/useDownloadManager';
import { formatBytes, formatTime } from '../../utils/format';
import DownloadQueuePopover from './DownloadQueuePopover';
import styles from './GlobalDownloadStatus.module.css';

interface GlobalDownloadStatusProps {
  /** Current download progress from useDownloadManager hook */
  progress: DownloadProgressView | null;
  /** Queue status from useDownloadManager hook */
  queueStatus: DownloadQueueStatus | null;
  /** Callback to cancel the current download */
  onCancel: (modelId: string) => void;
  /** Callback when user dismisses completion message */
  onDismiss: () => void;
  /** Callback to refresh queue status */
  onRefreshQueue?: () => void;
}

/**
 * Global download status component for page-level display.
 * Shows:
 * - Active download progress with shard support
 * - Queue status (X more queued)
 * - Completion summary with list of downloaded models (dismissible)
 */
const GlobalDownloadStatus: FC<GlobalDownloadStatusProps> = ({
  progress,
  queueStatus,
  onCancel,
  onDismiss,
  onRefreshQueue,
}) => {
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  // Track completed downloads since last dismiss
  const [completedModels, setCompletedModels] = useState<string[]>([]);
  const [showCompletion, setShowCompletion] = useState(false);
  const previousProgressRef = useRef<DownloadProgressView | null>(null);
  
  const currentId = queueStatus?.current?.id || progress?.id || '';
  const queueCount = queueStatus?.pending?.length || 0;
  const isActive = !!currentId;

  // Track when downloads complete
  useEffect(() => {
    const prev = previousProgressRef.current;
    
    // If we just went from an active state to completed
    if (progress?.status === 'completed' && prev?.status !== 'completed' && progress.id) {
      // Extract just the model name from the full ID (e.g., "unsloth/Qwen3-30B-A3B-GGUF:Q4_K_M" -> "Qwen3-30B-A3B-GGUF (Q4_K_M)")
      const [repoId, quant] = progress.id.split(':');
      const modelName = repoId.split('/').pop() || repoId;
      const displayName = quant ? `${modelName} (${quant})` : modelName;
      
      setCompletedModels(prev => {
        if (!prev.includes(displayName)) {
          return [...prev, displayName];
        }
        return prev;
      });
    }
    
    previousProgressRef.current = progress;
  }, [progress]);

  // Show completion message when queue empties and we have completed models
  useEffect(() => {
    const isQueueEmpty = !queueStatus?.current && (!queueStatus?.pending || queueStatus.pending.length === 0);
    const hasCompletedModels = completedModels.length > 0;
    
    if (isQueueEmpty && hasCompletedModels) {
      setShowCompletion(true);
    }
  }, [queueStatus, completedModels]);

  const handleDismiss = useCallback(() => {
    setShowCompletion(false);
    setCompletedModels([]);
    onDismiss();
  }, [onDismiss]);

  // Show completion summary
  if (showCompletion && !isActive) {
    return (
      <div className={styles.container}>
        <div className={styles.completionSection}>
          <div className={styles.completionHeader}>
            <span className={styles.completionIcon}>✅</span>
            <span className={styles.completionTitle}>
              {completedModels.length === 1 
                ? 'Download Complete'
                : `${completedModels.length} Downloads Complete`}
            </span>
          </div>
          <div className={styles.completedList}>
            {completedModels.map((name, idx) => (
              <div key={idx} className={styles.completedItem}>
                📦 {name}
              </div>
            ))}
          </div>
          <button 
            className={styles.dismissBtn}
            onClick={handleDismiss}
          >
            OK
          </button>
        </div>
      </div>
    );
  }

  if (!isActive) return null;

  const percentage = progress?.percentage ?? undefined;
  const shard = progress?.shard;
  const isSharded = !!(shard && shard.total > 1);

  return (
    <div className={styles.container}>
      <div className={styles.progressSection}>
        <div className={styles.progressHeader}>
          <div className={styles.progressInfo}>
            <span className={styles.statusIcon}>📥</span>
            <span className={styles.statusText}>
              {isSharded && shard ? `Downloading shard ${shard.index + 1}/${shard.total}` : 'Downloading'}
            </span>
            {queueCount > 0 && (
              <div className={styles.queueBadgeContainer}>
                <button
                  className={styles.queueBadge}
                  onClick={() => setIsQueuePopoverOpen((prev) => !prev)}
                  title="Click to view and manage queue"
                >
                  +{queueCount} queued
                </button>
                <DownloadQueuePopover
                  isOpen={isQueuePopoverOpen}
                  onClose={() => setIsQueuePopoverOpen(false)}
                  pendingItems={queueStatus?.pending || []}
                  onRefresh={onRefreshQueue}
                />
              </div>
            )}
          </div>
          {currentId && (
            <button className={styles.cancelBtn} onClick={() => onCancel(currentId)}>
              Cancel
            </button>
          )}
        </div>

        <div className={styles.modelName} title={currentId}>
          {currentId.length > 50 ? `${currentId.substring(0, 47)}...` : currentId}
        </div>

        <div className={styles.progressBarContainer}>
          <div className={styles.progressBar}>
            <div
              className={`${styles.progressBarFill} ${percentage === undefined ? styles.indeterminate : ''}`}
              style={percentage !== undefined ? { width: `${percentage}%` } : {}}
            />
          </div>
          <span className={styles.percentageText}>
            {percentage !== undefined ? `${percentage.toFixed(1)}%` : '...'}
          </span>
        </div>

        <div className={styles.statsRow}>
          {progress?.downloaded !== undefined && progress?.total !== undefined && (
            <span className={styles.stat}>
              {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
            </span>
          )}
          {progress?.speedBps !== undefined && (
            <span className={styles.stat}>{formatBytes(progress.speedBps)}/s</span>
          )}
          {progress?.etaSeconds !== undefined && (
            <span className={styles.stat}>ETA: {formatTime(progress.etaSeconds)}</span>
          )}
        </div>

        {isSharded && shard && (
          <div className={styles.shardSection}>
            <div className={styles.shardHeader}>
              <span className={styles.shardLabel}>
                Shard {shard.index + 1}/{shard.total}
              </span>
              {shard.filename && (
                <span className={styles.shardFilename} title={shard.filename}>
                  {shard.filename.length > 25 ? `...${shard.filename.slice(-22)}` : shard.filename}
                </span>
              )}
            </div>
            <div className={styles.shardProgressBar}>
              <div
                className={styles.shardProgressFill}
                style={{
                  width:
                    shard.totalBytes && shard.totalBytes > 0
                      ? `${((shard.downloaded || 0) / shard.totalBytes) * 100}%`
                      : '0%',
                }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default GlobalDownloadStatus;
