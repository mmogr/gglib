import { FC } from "react";
import {
  queueDownload,
  removeFromDownloadQueue,
  cancelShardGroup,
  clearFailedDownloads,
} from "../../services/tauri";
import { useDownloadProgress } from "../../hooks/useDownloadProgress";
import { useDownloadForm, useQueueActions } from "./hooks";
import DownloadForm from "./DownloadForm";
import CurrentDownloadProgress from "./CurrentDownloadProgress";
import DownloadQueue from "./DownloadQueue";

interface DownloadModelProps {
  onModelDownloaded: () => void;
}

/**
 * Download model orchestrator component.
 * 
 * Wires together:
 * - useDownloadProgress (shared progress/queue state)
 * - useDownloadForm (form state and submission)
 * - useQueueActions (queue manipulation handlers)
 * 
 * Renders:
 * - DownloadForm (repo ID, quantization, submit)
 * - CurrentDownloadProgress (progress bars, metrics, cancel)
 * - DownloadQueue (pending + failed lists)
 */
const DownloadModel: FC<DownloadModelProps> = ({ onModelDownloaded }) => {
  // Shared download progress and queue state
  const {
    progress,
    queueStatus,
    connectionMode,
    error,
    setError,
    fetchQueueStatus,
    cancelDownload,
    isDownloading,
    queueCount,
  } = useDownloadProgress({ onCompleted: onModelDownloaded });

  // Form state and submission logic
  const form = useDownloadForm({
    queueDownload,
    queueStatus,
    fetchQueueStatus,
    setError,
  });

  // Queue action handlers
  const queueActions = useQueueActions({
    removeFromDownloadQueue,
    cancelShardGroup,
    clearFailedDownloads,
    queueDownload,
    cancelDownload,
    fetchQueueStatus,
    setError,
  });

  const handleCancelCurrent = () => {
    if (queueStatus?.current) {
      queueActions.handleCancel(queueStatus.current.id);
    }
  };

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

      <DownloadForm
        repoId={form.repoId}
        setRepoId={form.setRepoId}
        quantization={form.quantization}
        setQuantization={form.setQuantization}
        submitting={form.submitting}
        canSubmit={form.canSubmit}
        isDownloading={isDownloading}
        error={error}
        onSubmit={form.handleSubmit}
      />

      {progress && (
        <CurrentDownloadProgress
          progress={progress}
          onCancel={handleCancelCurrent}
        />
      )}

      {queueStatus && (
        <DownloadQueue
          queueStatus={queueStatus}
          queueActions={queueActions}
        />
      )}
    </div>
  );
};

export default DownloadModel;
