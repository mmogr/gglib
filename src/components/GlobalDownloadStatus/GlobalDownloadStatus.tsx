import { FC, useState, useEffect, useRef, useCallback } from 'react';
import { DownloadProgress } from '../../hooks/useDownloadProgress';
import { DownloadQueueStatus } from '../../types';
import { formatBytes, formatTime } from '../../utils/format';
import DownloadQueuePopover from './DownloadQueuePopover';
import { ConfirmCancelModal } from './ConfirmCancelModal';
import styles from './GlobalDownloadStatus.module.css';

interface GlobalDownloadStatusProps {
  /** Current download progress from useDownloadProgress hook */
  progress: DownloadProgress | null;
  /** Queue status from useDownloadProgress hook */
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
  // Track completed downloads since last dismiss
  const [completedModels, setCompletedModels] = useState<string[]>([]);
  const [showCompletion, setShowCompletion] = useState(false);
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  const previousProgressRef = useRef<DownloadProgress | null>(null);

  // Cancel confirmation modal state
  const [showCancelModal, setShowCancelModal] = useState(false);
  const [pendingCancelModelId, setPendingCancelModelId] = useState<string | null>(null);
  const [isCancelling, setIsCancelling] = useState(false);

  // Track when downloads complete
  useEffect(() => {
    const prev = previousProgressRef.current;
    
    // If we just went from an active state to completed
    if (progress?.status === 'completed' && prev?.status !== 'completed' && progress.model_id) {
      // Extract just the model name from the full ID (e.g., "unsloth/Qwen3-30B-A3B-GGUF:Q4_K_M" -> "Qwen3-30B-A3B-GGUF (Q4_K_M)")
      const [repoId, quant] = progress.model_id.split(':');
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

  // Handle queue badge click
  const handleQueueBadgeClick = useCallback(() => {
    setIsQueuePopoverOpen((prev) => !prev);
  }, []);

  const handleClosePopover = useCallback(() => {
    setIsQueuePopoverOpen(false);
  }, []);

  const handleRefreshQueue = useCallback(() => {
    onRefreshQueue?.();
  }, [onRefreshQueue]);

  // Handle cancel button click - open confirmation modal
  // We need to define this before early returns to follow Rules of Hooks
  const handleCancelClick = useCallback((modelId: string) => {
    if (modelId) {
      setPendingCancelModelId(modelId);
      setShowCancelModal(true);
    }
  }, []);

  // Handle cancel confirmation
  const handleConfirmCancel = useCallback(async () => {
    if (!pendingCancelModelId) return;
    
    setIsCancelling(true);
    try {
      await onCancel(pendingCancelModelId);
    } finally {
      setIsCancelling(false);
      setShowCancelModal(false);
      setPendingCancelModelId(null);
    }
  }, [pendingCancelModelId, onCancel]);

  // Handle cancel modal dismissal
  const handleCancelModalClose = useCallback(() => {
    if (!isCancelling) {
      setShowCancelModal(false);
      setPendingCancelModelId(null);
    }
  }, [isCancelling]);

  // Get the current download from queue status (authoritative source)
  const currentDownload = queueStatus?.current;
  
  // The id in QueueSnapshot is already the canonical format (model_id:quantization)
  const currentDownloadFullId = currentDownload?.id || null;
  
  // Match progress events to current download with fallback for edge cases
  // Primary: exact match on canonical ID (model_id:quantization)
  // Fallback: match on base model_id prefix (handles timing edge cases)
  const isProgressMatch = progress && currentDownloadFullId && (
    progress.model_id === currentDownloadFullId ||
    progress.model_id.startsWith(currentDownload?.id.split(':')[0] + ':') ||
    currentDownloadFullId.startsWith(progress.model_id + ':')
  );
  
  const relevantProgress = isProgressMatch ? progress : null;

  // Determine if we should show anything
  const isActiveDownload = currentDownload || (progress && (
    progress.status === 'started' ||
    progress.status === 'downloading' ||
    progress.status === 'progress' ||
    progress.status === 'queued'
  ));
  
  const queueCount = (queueStatus?.pending?.length || 0);

  // Nothing to show
  if (!isActiveDownload && !showCompletion) {
    return null;
  }

  // Show completion summary
  if (showCompletion && !isActiveDownload) {
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

  // Get display values - prefer queue status (with full ID), fall back to progress
  const displayModelId = currentDownloadFullId || progress?.model_id || '';
  const displayProgress = relevantProgress;
  const isQueued = !currentDownload && progress?.status === 'queued';

  // Get shard info for the modal
  const shardInfo = displayProgress?.shard_progress;
  const isSharded = shardInfo ? shardInfo.total_shards > 1 : false;

  // Show active download progress
  return (
    <div className={styles.container}>
      <div className={styles.progressSection}>
        {/* Header with model name and cancel */}
        <div className={styles.progressHeader}>
          <div className={styles.progressInfo}>
            <span className={styles.statusIcon}>
              {isQueued ? '🕐' : '📥'}
            </span>
            <span className={styles.statusText}>
              {isQueued 
                ? 'Queued'
                : displayProgress?.shard_progress && displayProgress.shard_progress.total_shards > 1
                  ? `Downloading shard ${displayProgress.shard_progress.current_shard + 1}/${displayProgress.shard_progress.total_shards}`
                  : 'Downloading'}
            </span>
            {queueCount > 0 && (
              <div className={styles.queueBadgeContainer}>
                <button
                  className={styles.queueBadge}
                  onClick={handleQueueBadgeClick}
                  title="Click to view and manage queue"
                >
                  +{queueCount} queued
                </button>
                <DownloadQueuePopover
                  isOpen={isQueuePopoverOpen}
                  onClose={handleClosePopover}
                  pendingItems={queueStatus?.pending || []}
                  onRefresh={handleRefreshQueue}
                />
              </div>
            )}
          </div>
          {displayModelId && !isQueued && (
            <button
              className={styles.cancelBtn}
              onClick={() => handleCancelClick(displayModelId)}
            >
              Cancel
            </button>
          )}
        </div>

        {/* Model name */}
        <div className={styles.modelName} title={displayModelId}>
          {displayModelId.length > 50
            ? displayModelId.substring(0, 47) + '...'
            : displayModelId}
        </div>

        {/* Progress bar (only for active downloads, not queued) */}
        {!isQueued && (
          <>
            <div className={styles.progressBarContainer}>
              <div className={styles.progressBar}>
                <div
                  className={`${styles.progressBarFill} ${!displayProgress?.percentage ? styles.indeterminate : ''}`}
                  style={displayProgress?.percentage !== undefined ? { width: `${displayProgress.percentage}%` } : {}}
                />
              </div>
              <span className={styles.percentageText}>
                {displayProgress?.percentage !== undefined ? `${displayProgress.percentage.toFixed(1)}%` : '...'}
              </span>
            </div>

            {/* Stats row */}
            <div className={styles.statsRow}>
              {displayProgress?.downloaded !== undefined && displayProgress?.total !== undefined && (
                <span className={styles.stat}>
                  {formatBytes(displayProgress.downloaded)} / {formatBytes(displayProgress.total)}
                </span>
              )}
              {displayProgress?.speed !== undefined && (
                <span className={styles.stat}>
                  {formatBytes(displayProgress.speed)}/s
                </span>
              )}
              {displayProgress?.eta !== undefined && (
                <span className={styles.stat}>
                  ETA: {formatTime(displayProgress.eta)}
                </span>
              )}
            </div>

            {/* Shard progress for sharded downloads */}
            {displayProgress?.shard_progress && displayProgress.shard_progress.total_shards > 1 && (
              <div className={styles.shardSection}>
                <div className={styles.shardHeader}>
                  <span className={styles.shardLabel}>
                    Shard {displayProgress.shard_progress.current_shard + 1}/{displayProgress.shard_progress.total_shards}
                  </span>
                  <span className={styles.shardFilename} title={displayProgress.shard_progress.current_filename}>
                    {displayProgress.shard_progress.current_filename.length > 25
                      ? '...' + displayProgress.shard_progress.current_filename.slice(-22)
                      : displayProgress.shard_progress.current_filename}
                  </span>
                </div>
                <div className={styles.shardProgressBar}>
                  <div
                    className={styles.shardProgressFill}
                    style={{
                      width: displayProgress.shard_progress.shard_total > 0
                        ? `${(displayProgress.shard_progress.shard_downloaded / displayProgress.shard_progress.shard_total) * 100}%`
                        : '0%',
                    }}
                  />
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Cancel Confirmation Modal */}
      <ConfirmCancelModal
        isOpen={showCancelModal}
        modelId={pendingCancelModelId || ''}
        isSharded={isSharded}
        shardCount={shardInfo?.total_shards}
        currentShard={shardInfo ? shardInfo.current_shard + 1 : undefined}
        isCancelling={isCancelling}
        onConfirm={handleConfirmCancel}
        onCancel={handleCancelModalClose}
      />
    </div>
  );
};

export default GlobalDownloadStatus;
