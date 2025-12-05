import { FC } from 'react';
import type { DownloadProgressView } from '../../download/hooks/useDownloadManager';
import DownloadProgressBar from '../../download/components/DownloadProgressBar';
import ShardProgressIndicator from '../../download/components/ShardProgressIndicator';
import { formatBytes, formatTime } from '../../utils/format';
import styles from './DownloadProgressDisplay.module.css';

interface DownloadProgressDisplayProps {
  progress: DownloadProgressView;
  onCancel?: () => void;
  compact?: boolean;
  className?: string;
}

/**
 * Reusable download progress display component.
 * Shows progress bar, speed, ETA, and cancel button.
 */
const DownloadProgressDisplay: FC<DownloadProgressDisplayProps> = ({
  progress,
  onCancel,
  compact = false,
  className,
}) => {
  const isActive = progress.status === 'started' || progress.status === 'progress';
  const shard = progress.shard && progress.shard.total > 1 ? progress.shard : null;
  const percentage = progress.percentage ?? undefined;

  const statusIcon = progress.status === 'completed' ? '✅ ' : progress.status === 'error' ? '❌ ' : '📥 ';
  const statusText = progress.status === 'progress'
    ? shard
      ? `Downloading shard ${shard.index + 1}/${shard.total}... ${percentage?.toFixed(1) || 0}%`
      : `Downloading... ${percentage?.toFixed(1) || 0}%`
    : progress.message || 'Preparing download';

  return (
    <div className={`${styles.progressContainer} ${compact ? styles.compact : ''} ${className || ''}`}>
      <div className={styles.progressHeader}>
        <div className={styles.progressInfo}>
          <div className={styles.progressStatus}>{statusIcon}<span>{statusText}</span></div>
          {isActive && (
            <div className={styles.currentModelId} title={progress.id}>
              {progress.id.length > 40
                ? progress.id.substring(0, 37) + '...'
                : progress.id}
            </div>
          )}
        </div>
        {isActive && onCancel && (
          <button
            type="button"
            className={styles.cancelBtn}
            onClick={onCancel}
          >
            Cancel
          </button>
        )}
      </div>

      {isActive && (
        <div className={styles.progressBarContainer}>
          <DownloadProgressBar percentage={percentage} indeterminate={percentage === undefined} />

          {shard && (
            <ShardProgressIndicator
              shardLabel={`Shard ${shard.index + 1}/${shard.total}`}
              filename={shard.filename}
              downloaded={shard.downloaded}
              total={shard.totalBytes}
            />
          )}

          {percentage !== undefined && (
            compact ? (
              <div className={styles.compactStats}>
                {progress.downloaded !== undefined && progress.total !== undefined && (
                  <span>{formatBytes(progress.downloaded)} / {formatBytes(progress.total)}</span>
                )}
                {progress.speed_bps !== undefined && <span>{formatBytes(progress.speed_bps)}/s</span>}
                {progress.eta_seconds !== undefined && <span>ETA: {formatTime(progress.eta_seconds)}</span>}
              </div>
            ) : (
              <div className={styles.progressDetails}>
                <div>
                  <span className={styles.progressLabel}>{shard ? 'Overall' : 'Progress'}</span>
                  <span className={styles.progressPercentage}>{percentage.toFixed(1)}%</span>
                </div>
                {progress.downloaded !== undefined && progress.total !== undefined && (
                  <div>
                    <span className={styles.progressLabel}>{shard ? 'Total Size' : 'Size'}</span>
                    <span className={styles.progressMetric}>
                      {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                    </span>
                  </div>
                )}
                {progress.speed_bps !== undefined && (
                  <div>
                    <span className={styles.progressLabel}>Speed</span>
                    <span className={styles.progressMetric}>{formatBytes(progress.speed_bps)}/s</span>
                  </div>
                )}
                {progress.eta_seconds !== undefined && (
                  <div>
                    <span className={styles.progressLabel}>ETA</span>
                    <span className={styles.progressMetric}>{formatTime(progress.eta_seconds)}</span>
                  </div>
                )}
              </div>
            )
          )}
        </div>
      )}
    </div>
  );
};

export default DownloadProgressDisplay;
