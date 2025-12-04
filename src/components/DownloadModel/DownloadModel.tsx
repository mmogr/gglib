import { useState, FC, FormEvent } from "react";
import {
  queueDownload,
  removeFromDownloadQueue,
  cancelShardGroup,
  clearFailedDownloads,
} from "../../services/tauri";
import { DownloadQueueItem } from "../../types";
import { useDownloadProgress } from "../../hooks/useDownloadProgress";
import { formatBytes, formatTime } from "../../utils/format";
import styles from "./DownloadModel.module.css";

interface DownloadModelProps {
  onModelDownloaded: () => void;
}

const DownloadModel: FC<DownloadModelProps> = ({ onModelDownloaded }) => {
  const [repoId, setRepoId] = useState("");
  const [quantization, setQuantization] = useState("");
  const [submitting, setSubmitting] = useState(false);

  // Use the shared download progress hook
  const {
    progress,
    queueStatus,
    connectionMode,
    error,
    setError,
    fetchQueueStatus,
    isDownloading,
    queueCount,
    cancelDownload,
  } = useDownloadProgress({ onCompleted: onModelDownloaded });

  const commonQuantizations = [
    "Q4_0", "Q4_1", "Q5_0", "Q5_1", "Q8_0",
    "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L",
    "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M",
    "Q6_K", "Q8_K"
  ];

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!repoId.trim()) {
      setError("Please provide a repository ID");
      return;
    }

    // Check if queue is full
    if (queueStatus) {
      const currentCount = (queueStatus.current ? 1 : 0) + queueStatus.pending.length;
      if (currentCount >= queueStatus.max_size) {
        setError(`Queue is full (max ${queueStatus.max_size}). Please wait for a download to complete.`);
        return;
      }
    }

    try {
      setSubmitting(true);
      setError(null);
      const trimmedRepoId = repoId.trim();
      
      await queueDownload(
        trimmedRepoId,
        quantization || undefined,
      );
      
      // Clear form after successful queue
      setRepoId("");
      setQuantization("");
      
      // Refresh queue status
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to queue download");
    } finally {
      setSubmitting(false);
    }
  };

  const handleRemoveFromQueue = async (modelId: string) => {
    try {
      await removeFromDownloadQueue(modelId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove from queue");
    }
  };

  const handleCancelShardGroup = async (groupId: string) => {
    try {
      await cancelShardGroup(groupId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel shard group");
    }
  };

  const handleClearFailed = async () => {
    try {
      await clearFailedDownloads();
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear failed downloads");
    }
  };

  const handleRetry = async (item: DownloadQueueItem) => {
    try {
      // First remove from failed, then re-queue
      await removeFromDownloadQueue(item.model_id);
      await queueDownload(
        item.model_id,
        item.quantization || undefined,
      );
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to retry download");
    }
  };

  const handleCancel = async () => {
    if (!queueStatus?.current) {
      return;
    }
    // Use the hook's cancelDownload which handles state updates
    await cancelDownload(queueStatus.current.model_id);
  };

  const hasFailedDownloads = (queueStatus?.failed.length || 0) > 0;

  return (
    <div className="download-model-container">
      <h2>Download from HuggingFace</h2>
      <div style={{ fontSize: '0.8em', color: '#666', marginBottom: '10px' }}>
        Mode: {connectionMode}
        {queueCount > 0 && (
          <span style={{ marginLeft: '12px' }}>
            {queueCount} {queueCount === 1 ? 'download' : 'downloads'} queued
          </span>
        )}
      </div>

      <form onSubmit={handleSubmit} className="download-form">
        <div className="form-group">
          <label htmlFor="repoId">Repository ID:</label>
          <input
            type="text"
            id="repoId"
            value={repoId}
            onChange={(e) => setRepoId(e.target.value)}
            placeholder="e.g. microsoft/DialoGPT-medium"
            className="form-input"
            required
            disabled={submitting}
          />
          <small className="form-hint">
            Format: username/repository-name or organization/repository-name
          </small>
        </div>

        <div className="form-group">
          <label htmlFor="quantization">Quantization (optional):</label>
          <input
            type="text"
            id="quantization"
            list="quantization-options"
            value={quantization}
            onChange={(e) => setQuantization(e.target.value)}
            placeholder="Auto-detect or enter custom (e.g., Q4_K_M)"
            className="form-input"
            disabled={submitting}
          />
          <datalist id="quantization-options">
            {commonQuantizations.map((quant) => (
              <option key={quant} value={quant} />
            ))}
          </datalist>
          <small className="form-hint">
            Select from common options or type your own quantization format
          </small>
        </div>

        {error && <div className="error-message">{error}</div>}

        <div className="form-actions">
          <button
            type="submit"
            disabled={submitting || !repoId.trim() || (queueStatus !== null && queueCount >= queueStatus.max_size)}
            className="btn btn-primary"
          >
            {submitting ? "Adding to Queue..." : isDownloading ? "Add to Queue" : "Download Model"}
          </button>
        </div>
      </form>

      {/* Current Download Progress */}
      {progress && (
        <div className={styles.progressContainer}>
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
              {/* Show model ID for downloading status */}
              {(progress.status === 'started' || progress.status === 'downloading' || progress.status === 'progress') && (
                <div className={styles.currentModelId} title={progress.model_id}>
                  {progress.model_id.length > 40 
                    ? progress.model_id.substring(0, 37) + '...' 
                    : progress.model_id}
                </div>
              )}
            </div>
            {(progress.status === 'started' || progress.status === 'downloading' || progress.status === 'progress') && (
              <button
                type="button"
                className={`btn btn-sm ${styles.cancelBtn}`}
                onClick={handleCancel}
              >
                Cancel
              </button>
            )}
          </div>
          {(progress.status === 'started' || progress.status === 'downloading' || progress.status === 'progress') && (
            <div className={styles.progressBarContainer}>
              {/* Overall progress bar for sharded downloads */}
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
                        width: progress.shard_progress.shard_total > 0 
                          ? `${(progress.shard_progress.shard_downloaded / progress.shard_progress.shard_total) * 100}%` 
                          : '0%' 
                      }}
                    ></div>
                  </div>
                  <div className={styles.shardProgressDetails}>
                    <span>
                      {formatBytes(progress.shard_progress.shard_downloaded)} / {formatBytes(progress.shard_progress.shard_total)}
                    </span>
                  </div>
                </div>
              )}

              {progress.percentage !== undefined && (
                <div className={styles.progressDetails}>
                  <div>
                    <span className={styles.progressLabel}>
                      {progress.shard_progress && progress.shard_progress.total_shards > 1 ? 'Overall' : 'Progress'}
                    </span>
                    <span className={styles.progressPercentage}>{progress.percentage.toFixed(1)}%</span>
                  </div>
                  {progress.downloaded !== undefined && progress.total !== undefined && (
                    <div>
                      <span className={styles.progressLabel}>
                        {progress.shard_progress && progress.shard_progress.total_shards > 1 ? 'Total Size' : 'Size'}
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
      )}

      {/* Download Queue */}
      {queueStatus && (queueStatus.pending.length > 0 || hasFailedDownloads) && (
        <div className={styles.queueSection}>
          {/* Pending Queue */}
          {queueStatus.pending.length > 0 && (
            <>
              <h3 className={styles.queueTitle}>
                Queued Downloads ({queueStatus.pending.length})
              </h3>
              <ul className={styles.queueList}>
                {queueStatus.pending.map((item, index) => {
                  const isShard = item.shard_info !== null && item.shard_info !== undefined;
                  const shardLabel = isShard
                    ? `shard ${item.shard_info!.shard_index + 1}/${item.shard_info!.total_shards}`
                    : null;
                  
                  return (
                    <li key={`${item.model_id}-${index}`} className={styles.queueItem}>
                      <div className={styles.queueItemInfo}>
                        <span className={styles.queuePosition}>#{item.position}</span>
                        <span className={styles.queueModelId}>
                          {item.model_id}
                          {shardLabel && (
                            <span className={styles.shardBadge}>{shardLabel}</span>
                          )}
                        </span>
                        {item.quantization && (
                          <span className={styles.queueQuant}>{item.quantization}</span>
                        )}
                      </div>
                      <button
                        type="button"
                        className={`btn btn-sm ${styles.removeBtn}`}
                        onClick={() => item.group_id 
                          ? handleCancelShardGroup(item.group_id)
                          : handleRemoveFromQueue(item.model_id)
                        }
                        aria-label={item.group_id 
                          ? `Cancel all shards for ${item.model_id}`
                          : `Remove ${item.model_id} from queue`
                        }
                        title={item.group_id ? "Cancel all shards" : "Remove from queue"}
                      >
                        {item.group_id ? "Cancel All" : "✕"}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </>
          )}

          {/* Failed Downloads */}
          {hasFailedDownloads && (
            <>
              <div className={styles.failedHeader}>
                <h3 className={styles.queueTitle}>
                  Failed Downloads ({queueStatus.failed.length})
                </h3>
                <button
                  type="button"
                  className={`btn btn-sm ${styles.clearFailedBtn}`}
                  onClick={handleClearFailed}
                >
                  Clear All
                </button>
              </div>
              <ul className={styles.queueList}>
                {queueStatus.failed.map((item, index) => {
                  const isShard = item.shard_info !== null && item.shard_info !== undefined;
                  const shardLabel = isShard
                    ? `shard ${item.shard_info!.shard_index + 1}/${item.shard_info!.total_shards}`
                    : null;

                  return (
                    <li key={`failed-${item.model_id}-${index}`} className={`${styles.queueItem} ${styles.queueItemFailed}`}>
                      <div className={styles.queueItemInfo}>
                        <span className={styles.failedIcon}>❌</span>
                        <span className={styles.queueModelId}>
                          {item.model_id}
                          {shardLabel && (
                            <span className={styles.shardBadge}>{shardLabel}</span>
                          )}
                        </span>
                        {item.quantization && (
                          <span className={styles.queueQuant}>{item.quantization}</span>
                        )}
                        {item.error && (
                          <span className={styles.errorText} title={item.error}>
                            {item.error.length > 40 ? item.error.substring(0, 40) + '...' : item.error}
                          </span>
                        )}
                      </div>
                      <div className={styles.failedActions}>
                        <button
                          type="button"
                          className={`btn btn-sm ${styles.retryBtn}`}
                          onClick={() => handleRetry(item)}
                          aria-label={`Retry ${item.model_id}`}
                        >
                          Retry
                        </button>
                        <button
                          type="button"
                          className={`btn btn-sm ${styles.removeBtn}`}
                          onClick={() => handleRemoveFromQueue(item.model_id)}
                          aria-label={`Remove ${item.model_id} from failed list`}
                        >
                          ✕
                        </button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </>
          )}
        </div>
      )}
    </div>
  );
};

export default DownloadModel;