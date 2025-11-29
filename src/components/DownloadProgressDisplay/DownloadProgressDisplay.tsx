import { FC } from 'react';
import { DownloadProgress } from '../../hooks/useDownloadProgress';
import { formatBytes, formatTime } from '../../utils/format';
import styles from './DownloadProgressDisplay.module.css';

interface DownloadProgressDisplayProps {
  progress: DownloadProgress;
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
  const isActive =
    progress.status === 'started' ||
    progress.status === 'downloading' ||
    progress.status === 'progress';

  return (
    <div className={`${styles.progressContainer} ${compact ? styles.compact : ''} ${className || ''}`}>
      <div className={styles.progressHeader}>
        <div className={styles.progressInfo}>
          <div className={styles.progressStatus}>
            {progress.status === 'started' && '⏳ '}
            {(progress.status === 'downloading' || progress.status === 'progress') && '📥 '}
            {progress.status === 'completed' && '✅ '}
            {progress.status === 'error' && '❌ '}
            {progress.status === 'queued' && '🕐 '}
            {progress.status === 'skipped' && '⏭️ '}
            <span>
              {(progress.status === 'downloading' || progress.status === 'progress')
                ? progress.shard_progress && progress.shard_progress.total_shards > 1
                  ? `Downloading shard ${progress.shard_progress.current_shard + 1}/${progress.shard_progress.total_shards}... ${progress.percentage?.toFixed(1) || 0}%`
                  : `Downloading... ${progress.percentage?.toFixed(1) || 0}%`
                : progress.message}
            </span>
          </div>
          {isActive && (
            <div className={styles.currentModelId} title={progress.model_id}>
              {progress.model_id.length > 40
                ? progress.model_id.substring(0, 37) + '...'
                : progress.model_id}
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
          {/* Main progress bar */}
          <div className={styles.progressBar}>
            <div
              className={`${styles.progressBarFill} ${progress.percentage !== undefined ? '' : styles.indeterminate}`}
              style={progress.percentage !== undefined ? { width: `${progress.percentage}%` } : {}}
            ></div>
          </div>

          {/* Shard-specific progress for sharded downloads */}
          {progress.shard_progress && progress.shard_progress.total_shards > 1 && (
            <div className={styles.shardProgressSection}>
              <div className={styles.shardProgressHeader}>
                <span className={styles.shardLabel}>
                  Shard {progress.shard_progress.current_shard + 1}/{progress.shard_progress.total_shards}
                </span>
                <span className={styles.shardFilename} title={progress.shard_progress.current_filename}>
                  {progress.shard_progress.current_filename.length > 30
                    ? '...' + progress.shard_progress.current_filename.slice(-27)
                    : progress.shard_progress.current_filename}
                </span>
              </div>
              <div className={styles.shardProgressBar}>
                <div
                  className={styles.shardProgressBarFill}
                  style={{
                    width:
                      progress.shard_progress.shard_total > 0
                        ? `${(progress.shard_progress.shard_downloaded / progress.shard_progress.shard_total) * 100}%`
                        : '0%',
                  }}
                ></div>
              </div>
              <div className={styles.shardProgressDetails}>
                <span>
                  {formatBytes(progress.shard_progress.shard_downloaded)} /{' '}
                  {formatBytes(progress.shard_progress.shard_total)}
                </span>
              </div>
            </div>
          )}

          {/* Progress details */}
          {!compact && progress.percentage !== undefined && (
            <div className={styles.progressDetails}>
              <div>
                <span className={styles.progressLabel}>
                  {progress.shard_progress && progress.shard_progress.total_shards > 1
                    ? 'Overall'
                    : 'Progress'}
                </span>
                <span className={styles.progressPercentage}>{progress.percentage.toFixed(1)}%</span>
              </div>
              {progress.downloaded !== undefined && progress.total !== undefined && (
                <div>
                  <span className={styles.progressLabel}>
                    {progress.shard_progress && progress.shard_progress.total_shards > 1
                      ? 'Total Size'
                      : 'Size'}
                  </span>
                  <span className={styles.progressMetric}>
                    {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
                  </span>
                </div>
              )}
              {progress.speed !== undefined && (
                <div>
                  <span className={styles.progressLabel}>Speed</span>
                  <span className={styles.progressMetric}>{formatBytes(progress.speed)}/s</span>
                </div>
              )}
              {progress.eta !== undefined && (
                <div>
                  <span className={styles.progressLabel}>ETA</span>
                  <span className={styles.progressMetric}>{formatTime(progress.eta)}</span>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default DownloadProgressDisplay;
