import { FC } from "react";
import type { DownloadProgressView } from "../../download/hooks/useDownloadManager";
import { formatBytes, formatTime } from "../../utils/format";
import styles from "./DownloadModel.module.css";

interface CurrentDownloadProgressProps {
  progress: DownloadProgressView;
  onCancel: () => void;
}

/**
 * Displays current download progress with:
 * - Status indicator and message
 * - Progress bar (overall and per-shard for sharded downloads)
 * - Speed, ETA, and size metrics
 * - Cancel button
 */
const CurrentDownloadProgress: FC<CurrentDownloadProgressProps> = ({
  progress,
  onCancel,
}) => {
  const isActive = progress.status === 'started' || progress.status === 'progress';

  return (
    <div className={styles.progressContainer}>
      <div className={styles.progressHeader}>
        <div className={styles.progressInfo}>
          <div className={styles.progressStatus}>
            {progress.status === 'started' && '⏳ '}
            {progress.status === 'progress' && '📥 '}
            {progress.status === 'completed' && '✅ '}
            {progress.status === 'error' && '❌ '}
            <span>
              {progress.status === 'progress'
                ? progress.shard && progress.shard.total > 1
                  ? `Downloading shard ${progress.shard.index + 1}/${progress.shard.total}... ${progress.percentage?.toFixed(1) || 0}%`
                  : `Downloading... ${progress.percentage?.toFixed(1) || 0}%`
                : progress.message}
            </span>
          </div>
          {/* Show model ID for active downloads */}
          {isActive && (
            <div className={styles.currentModelId} title={progress.id}>
              {progress.id.length > 40 
                ? progress.id.substring(0, 37) + '...' 
                : progress.id}
            </div>
          )}
        </div>
        {isActive && (
          <button
            type="button"
            className={`btn btn-sm ${styles.cancelBtn}`}
            onClick={onCancel}
          >
            Cancel
          </button>
        )}
      </div>

      {isActive && (
        <div className={styles.progressBarContainer}>
          {/* Overall progress bar */}
          <div className={styles.progressBar}>
            <div 
              className={`${styles.progressBarFill} ${progress.percentage !== undefined ? '' : styles.indeterminate}`}
              style={progress.percentage !== undefined ? { width: `${progress.percentage}%` } : {}}
            ></div>
          </div>
          
          {/* Shard-specific progress for sharded downloads */}
          {progress.shard && progress.shard.total > 1 && (
            <div className={styles.shardProgressSection}>
              <div className={styles.shardProgressHeader}>
                <span className={styles.shardLabel}>
                  Shard {progress.shard.index + 1}/{progress.shard.total}
                </span>
                <span className={styles.shardFilename} title={progress.shard.filename}>
                  {progress.shard.filename && progress.shard.filename.length > 30 
                    ? '...' + progress.shard.filename.slice(-27)
                    : progress.shard.filename}
                </span>
              </div>
              <div className={styles.shardProgressBar}>
                <div 
                  className={styles.shardProgressBarFill}
                  style={{ 
                    width: progress.shard.totalBytes && progress.shard.totalBytes > 0 
                      ? `${((progress.shard.downloaded || 0) / progress.shard.totalBytes) * 100}%` 
                      : '0%' 
                  }}
                ></div>
              </div>
              <div className={styles.shardProgressDetails}>
                <span>
                  {formatBytes(progress.shard.downloaded || 0)} / {formatBytes(progress.shard.totalBytes || 0)}
                </span>
              </div>
            </div>
          )}

          {/* Progress metrics */}
          {progress.percentage !== undefined && (
            <div className={styles.progressDetails}>
              <div>
                <span className={styles.progressLabel}>{progress.shard && progress.shard.total > 1 ? 'Overall' : 'Progress'}</span>
                <span className={styles.progressPercentage}>{progress.percentage.toFixed(1)}%</span>
              </div>
              {progress.downloaded !== undefined && progress.total !== undefined && (
                <div>
                  <span className={styles.progressLabel}>
                    {progress.shard && progress.shard.total > 1 ? 'Total Size' : 'Size'}
                  </span>
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
          )}
        </div>
      )}
    </div>
  );
};

export default CurrentDownloadProgress;
