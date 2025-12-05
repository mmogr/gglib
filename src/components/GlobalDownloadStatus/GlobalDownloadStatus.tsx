import { FC, useState } from 'react';
import type { DownloadQueueStatus } from '../../download/api/types';
import type { DownloadProgressView } from '../../download/hooks/useDownloadManager';
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
  /** Callback to refresh queue status */
  onRefreshQueue?: () => void;
}

/**
 * Global download status component for page-level display.
 * Shows:
 * - Active download progress with shard support
 * - Queue status (X more queued)
 */
const GlobalDownloadStatus: FC<GlobalDownloadStatusProps> = ({
  progress,
  queueStatus,
  onCancel,
  onRefreshQueue,
}) => {
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  const currentId = queueStatus?.current?.id || progress?.id || '';
  const queueCount = queueStatus?.pending.length || 0;
  const isActive = !!currentId;

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
          {progress?.speed_bps !== undefined && (
            <span className={styles.stat}>{formatBytes(progress.speed_bps)}/s</span>
          )}
          {progress?.eta_seconds !== undefined && (
            <span className={styles.stat}>ETA: {formatTime(progress.eta_seconds)}</span>
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

      {/* Completion dismissal not shown; parent can manage banners if desired */}
    </div>
  );
};

export default GlobalDownloadStatus;
