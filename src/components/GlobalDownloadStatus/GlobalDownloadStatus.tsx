import { FC, useState } from 'react';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import type { DownloadProgressView, DownloadUiState } from '../../hooks/useDownloadManager';
import type { QueueRunSummary } from '../../services/transport/types/events';
import { formatBytes, formatTime } from '../../utils/format';
import DownloadQueuePopover from './DownloadQueuePopover';
import styles from './GlobalDownloadStatus.module.css';

interface GlobalDownloadStatusProps {
  /** Current download progress from useDownloadManager hook */
  progress: DownloadProgressView | null;
  /** Queue status from useDownloadManager hook */
  queueStatus: DownloadQueueStatus | null;
  /** Single source of truth for UI state (replaces derived currentId logic) */
  downloadUiState: DownloadUiState;
  /** Summary of last completed queue run (null if none or dismissed) */
  lastQueueSummary: QueueRunSummary | null;
  /** Callback to cancel the current download */
  onCancel: (modelId: string) => void;
  /** Callback when user dismisses completion summary */
  onDismissSummary: () => void;
  /** Callback to refresh queue status */
  onRefreshQueue?: () => void;
}

/**
 * Global download status component for page-level display.
 * Shows:
 * - Active download progress with shard support
 * - Queue status (X more queued)
 * - Completion summary with ALL downloaded models from queue run (dismissible)
 */
const GlobalDownloadStatus: FC<GlobalDownloadStatusProps> = ({
  progress,
  queueStatus,
  downloadUiState,
  lastQueueSummary,
  onCancel,
  onDismissSummary,
  onRefreshQueue,
}) => {
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  
  // Single source of truth for what should be displayed
  const isActive = !!downloadUiState.activeId;
  const currentId = downloadUiState.activeId || '';
  const isCancelling = downloadUiState.phase === 'cancelling';
  const queueCount = queueStatus?.pending?.length || 0;

  // Show completion summary (priority over active progress)
  if (lastQueueSummary && !isActive) {
    const downloaded = lastQueueSummary.items.filter(
      (item) => item.last_result === 'downloaded'
    );
    const totalAttempts =
      lastQueueSummary.total_attempts_downloaded +
      lastQueueSummary.total_attempts_failed +
      lastQueueSummary.total_attempts_cancelled;
    const uniqueTotal = lastQueueSummary.unique_models_downloaded;
    const hasRetries = totalAttempts > uniqueTotal;

    // Only show banner if at least one model was downloaded
    if (uniqueTotal === 0) {
      return null;
    }

    // Show first 3 items from the downloaded list
    const displayItems = downloaded.slice(0, 3);
    const shownCount = displayItems.length;
    // Remaining = unique total minus what we're showing
    const remaining = Math.max(0, uniqueTotal - shownCount);

    return (
      <div className={styles.container}>
        <div className={styles.completionSection}>
          <div className={styles.completionHeader}>
            <span className={styles.completionIcon}>✅</span>
            <span className={styles.completionTitle}>
              {uniqueTotal === 1 ? 'Download Complete' : `${uniqueTotal} Downloads Complete`}
            </span>
          </div>
          <div className={styles.completedList}>
            {displayItems.length > 0 ? (
              <>
                {displayItems.map((item, idx) => (
                  <div key={idx} className={styles.completedItem}>
                    📦 {item.display_name}
                  </div>
                ))}
                {remaining > 0 && (
                  <div className={styles.completedItem}>
                    …and {remaining} more
                  </div>
                )}
              </>
            ) : (
              <div className={styles.completedItem}>
                📦 {uniqueTotal} {uniqueTotal === 1 ? 'model' : 'models'} downloaded
                {lastQueueSummary.truncated && ' (details truncated)'}
              </div>
            )}
          </div>
          {hasRetries && (
            <div className={styles.retryNotice}>
              🔁 {totalAttempts} total attempts
            </div>
          )}
          <button className={styles.dismissBtn} onClick={onDismissSummary}>
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
            <button 
              className={styles.cancelBtn} 
              onClick={() => onCancel(currentId)}
              disabled={isCancelling}
            >
              {isCancelling ? 'Cancelling...' : 'Cancel'}
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
