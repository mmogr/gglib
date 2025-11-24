import { useState, FC, FormEvent, useEffect } from "react";
import { TauriService } from "../services/tauri";
import styles from "./DownloadModel.module.css";

interface DownloadModelProps {
  onModelDownloaded: () => void;
}

interface DownloadProgress {
  status: "started" | "downloading" | "progress" | "completed" | "error";
  model_id: string;
  message?: string;
  progress?: number;
  downloaded?: number;
  total?: number;
  percentage?: number;
  speed?: number; // bytes per second
  eta?: number;   // seconds remaining
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
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [connectionMode, setConnectionMode] = useState<string>("Initializing...");
  const [activeDownloadId, setActiveDownloadId] = useState<string | null>(null);

  const commonQuantizations = [
    "Q4_0", "Q4_1", "Q5_0", "Q5_1", "Q8_0",
    "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L",
    "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M",
    "Q6_K", "Q8_K"
  ];

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

            if (progressData.status === 'completed' || progressData.status === 'error') {
              setActiveDownloadId(null);
            }

            if (progressData.status === 'completed') {
              setTimeout(() => {
                setProgress(null);
                setDownloading(false);
              }, 2000);
            } else if (progressData.status === 'error') {
              setDownloading(false);
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
        // Web Environment - Use SSE
        // Determine API base URL (same origin for production, localhost:9887 for dev)
        const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
        eventSource = new EventSource(`${baseUrl}/api/models/download/progress`);
        
        eventSource.onmessage = (event) => {
          try {
            const progressData = JSON.parse(event.data) as DownloadProgress;
            
            setProgress(progressData);

            if (progressData.status === 'completed' || progressData.status === 'error') {
              setActiveDownloadId(null);
            }

            if (progressData.status === 'completed') {
              setTimeout(() => {
                setProgress(null);
                setDownloading(false);
              }, 2000);
            } else if (progressData.status === 'error') {
              setDownloading(false);
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
  }, []);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!repoId.trim()) {
      setError("Please provide a repository ID");
      return;
    }

    try {
      setDownloading(true);
      setError(null);
      const trimmedRepoId = repoId.trim();
      setActiveDownloadId(trimmedRepoId);
      
      await TauriService.downloadModel({
        repo_id: trimmedRepoId,
        quantization: quantization || undefined,
      });
      
      setTimeout(() => {
        setRepoId("");
        setQuantization("");
      }, 500);
      setDownloading(false);
      setActiveDownloadId(null);
      
      onModelDownloaded();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to download model");
      setProgress({
        status: "error",
        model_id: repoId.trim(),
        message: err instanceof Error ? err.message : "Failed to download model"
      });
      setDownloading(false);
      setActiveDownloadId(null);
    }
  };

  const handleCancel = async () => {
    if (!activeDownloadId) {
      return;
    }

    try {
      await TauriService.cancelDownload(activeDownloadId);
      setError("Download cancelled");
      setProgress(null);
      setActiveDownloadId(null);
      setDownloading(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel download");
    }
  };

  return (
    <div className="download-model-container">
      <h2>Download from HuggingFace</h2>
      <div style={{ fontSize: '0.8em', color: '#666', marginBottom: '10px' }}>
        Mode: {connectionMode}
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

        {progress && (
          <div className={styles.progressContainer}>
            <div className={styles.progressStatus}>
              {progress.status === 'started' && '⏳ '}
              {(progress.status === 'downloading' || progress.status === 'progress') && '📥 '}
              {progress.status === 'completed' && '✅ '}
              {progress.status === 'error' && '❌ '}
              <span>{progress.message}</span>
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

        <div className="form-actions">
          <button
            type="submit"
            disabled={downloading || !repoId.trim()}
            className="btn btn-primary"
          >
            {downloading ? "Downloading..." : "Download Model"}
          </button>
          {downloading && (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={handleCancel}
              style={{ marginLeft: '8px' }}
            >
              Stop
            </button>
          )}
        </div>
      </form>
    </div>
  );
};

export default DownloadModel;