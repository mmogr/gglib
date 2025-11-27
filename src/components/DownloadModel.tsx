import { useState, FC, FormEvent, useEffect, useCallback } from "react";
import { TauriService } from "../services/tauri";
import { DownloadQueueStatus, DownloadQueueItem } from "../types";
import styles from "./DownloadModel.module.css";

interface DownloadModelProps {
  onModelDownloaded: () => void;
}

interface DownloadProgress {
  status: "started" | "downloading" | "progress" | "completed" | "error" | "queued" | "skipped";
  model_id: string;
  message?: string;
  progress?: number;
  downloaded?: number;
  total?: number;
  percentage?: number;
  speed?: number; // bytes per second
  eta?: number;   // seconds remaining
  queue_position?: number;
  queue_length?: number;
}

const formatBytes = (bytes: number, decimals = 2) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + ' ' + sizes[i];
};

const formatTime = (seconds: number) => {
  if (!isFinite(seconds) || seconds < 0) return 'Calculating...';
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.ceil(seconds % 60);
  return `${minutes}m ${remainingSeconds}s`;
};

const DownloadModel: FC<DownloadModelProps> = ({ onModelDownloaded }) => {
  const [repoId, setRepoId] = useState("");
  const [quantization, setQuantization] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [connectionMode, setConnectionMode] = useState<string>("Initializing...");
  const [queueStatus, setQueueStatus] = useState<DownloadQueueStatus | null>(null);

  const commonQuantizations = [
    "Q4_0", "Q4_1", "Q5_0", "Q5_1", "Q8_0",
    "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L",
    "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M",
    "Q6_K", "Q8_K"
  ];

  // Fetch the current queue status
  const fetchQueueStatus = useCallback(async () => {
    try {
      const status = await TauriService.getDownloadQueue();
      setQueueStatus(status);
    } catch (err) {
      console.error("Failed to fetch queue status:", err);
    }
  }, []);

  // Initial fetch and periodic refresh of queue status
  useEffect(() => {
    fetchQueueStatus();
    const interval = setInterval(fetchQueueStatus, 2000);
    return () => clearInterval(interval);
  }, [fetchQueueStatus]);

  // Listen for download progress events from Tauri or Web SSE
  useEffect(() => {
    let unlistenTauri: (() => void) | undefined;
    let eventSource: EventSource | undefined;

    const setupListener = async () => {
      // Check for Tauri environment (supports both v1 and v2)
      // We check for __TAURI_INTERNALS__ (v2) or __TAURI__ (v1)
      // We also check if the window has the IPC function which is the most reliable
      const isTauri = typeof (window as any).__TAURI_INTERNALS__ !== 'undefined' ||
                      typeof (window as any).__TAURI__ !== 'undefined' ||
                      typeof (window as any).__TAURI_IPC__ !== 'undefined';

      console.log("[DownloadModel] Environment check:", { isTauri });

      // Tauri Environment
      if (isTauri) {
        setConnectionMode("Desktop (Tauri)");
        try {
          console.log("[DownloadModel] Setting up Tauri event listener...");
          const { listen } = await import('@tauri-apps/api/event');
          unlistenTauri = await listen<DownloadProgress>('download-progress', (event) => {
            console.log("[DownloadModel] Received Tauri event:", event);
            const progressData = event.payload;
            
            setProgress(progressData);
            // Refresh queue status on any progress event
            fetchQueueStatus();

            if (progressData.status === 'completed') {
              onModelDownloaded();
              setTimeout(() => {
                setProgress(null);
              }, 2000);
            } else if (progressData.status === 'error' || progressData.status === 'skipped') {
              // Keep progress visible for errors but allow new submissions
            }
          });
          console.log("[DownloadModel] Tauri event listener registered.");
        } catch (e) {
          console.error("[DownloadModel] Failed to setup Tauri listener:", e);
          setConnectionMode(`Desktop Error: ${e instanceof Error ? e.message : String(e)}`);
        }
      } else {
        setConnectionMode("Web (SSE)");
        // Web Environment - Use SSE
        // Determine API base URL (same origin for production, localhost:9887 for dev)
        const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
        eventSource = new EventSource(`${baseUrl}/api/models/download/progress`);
        
        eventSource.onmessage = (event) => {
          try {
            const progressData = JSON.parse(event.data) as DownloadProgress;
            
            setProgress(progressData);
            // Refresh queue status on any progress event
            fetchQueueStatus();

            if (progressData.status === 'completed') {
              onModelDownloaded();
              setTimeout(() => {
                setProgress(null);
              }, 2000);
            } else if (progressData.status === 'error' || progressData.status === 'skipped') {
              // Keep progress visible for errors but allow new submissions
            }
          } catch (e) {
            console.error("Failed to parse progress event", e);
          }
        };

        eventSource.onerror = (err) => {
           console.error("SSE Error:", err);
           // Don't close immediately on error, it might reconnect
        };
      }
    };

    setupListener();

    return () => {
      if (unlistenTauri) {
        unlistenTauri();
      }
      if (eventSource) {
        eventSource.close();
      }
    };
  }, [fetchQueueStatus, onModelDownloaded]);

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
      
      await TauriService.queueDownload(
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
      await TauriService.removeFromDownloadQueue(modelId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove from queue");
    }
  };

  const handleCancelShardGroup = async (groupId: string) => {
    try {
      await TauriService.cancelShardGroup(groupId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel shard group");
    }
  };

  const handleClearFailed = async () => {
    try {
      await TauriService.clearFailedDownloads();
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear failed downloads");
    }
  };

  const handleRetry = async (item: DownloadQueueItem) => {
    try {
      // First remove from failed, then re-queue
      await TauriService.removeFromDownloadQueue(item.model_id);
      await TauriService.queueDownload(
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

    try {
      await TauriService.cancelDownload(queueStatus.current.model_id);
      setError("Download cancelled");
      setProgress(null);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel download");
    }
  };

  // Check if we're currently downloading
  const isDownloading = queueStatus?.current !== null && queueStatus?.current !== undefined;
  const queueCount = (queueStatus?.pending.length || 0) + (isDownloading ? 1 : 0);
  const hasFailedDownloads = (queueStatus?.failed.length || 0) > 0;

  return (
    <div className="download-model-container">
      <h2>Download from HuggingFace</h2>
      <div style={{ fontSize: '0.8em', color: '#666', marginBottom: '10px' }}>
        Mode: {connectionMode}
        {queueStatus && (
          <span style={{ marginLeft: '12px' }}>
            Queue: {queueCount}/{queueStatus.max_size}
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
                    ? `Downloading... ${progress.percentage?.toFixed(1) || 0}%`
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
              <div className={styles.progressBar}>
                <div 
                    className={`${styles.progressBarFill} ${progress.percentage !== undefined ? '' : styles.indeterminate}`}
                    style={progress.percentage !== undefined ? { width: `${progress.percentage}%` } : {}}
                ></div>
              </div>
              {progress.percentage !== undefined && (
                <div className={styles.progressDetails}>
                  <div>
                    <span className={styles.progressLabel}>Progress</span>
                    <span className={styles.progressPercentage}>{progress.percentage.toFixed(1)}%</span>
                  </div>
                  {progress.downloaded !== undefined && progress.total !== undefined && (
                    <div>
                      <span className={styles.progressLabel}>Size</span>
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
          )}}

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