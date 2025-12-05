import { FC } from "react";
import { removeFromDownloadQueue } from "../../download/api/downloadApi";
import { useDownloadManager } from "../../download/hooks/useDownloadManager";
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
 * - useDownloadManager (shared progress/queue state)
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
    currentProgress,
    queueStatus,
    connectionMode,
    error,
    setError,
    refreshQueue,
    cancel,
    cancelGroup,
    clearFailed,
    queueModel,
    isDownloading,
    queueLength,
  } = useDownloadManager({ onCompleted: onModelDownloaded });

  // Form state and submission logic
  const form = useDownloadForm({
    queueDownload: queueModel,
    queueStatus,
    refreshQueue,
    setError,
  });

  // Queue action handlers
  const queueActions = useQueueActions({
    removeFromDownloadQueue,
    cancelShardGroup: cancelGroup,
    clearFailedDownloads: clearFailed,
    queueDownload: queueModel,
    cancelDownload: cancel,
    fetchQueueStatus: refreshQueue,
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
        {queueLength > 0 && (
          <span style={{ marginLeft: '12px' }}>
            {queueLength} {queueLength === 1 ? 'download' : 'downloads'} queued
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

      {currentProgress && (
        <CurrentDownloadProgress
          progress={currentProgress}
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
