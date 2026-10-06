import { useCallback } from 'react';
import { appLogger } from '../../../services/platform';
import type { QueueSnapshot } from '../../../services/transport/types/downloads';
import { useToastContext } from '../../../contexts/ToastContext';
import { getTransport } from '../../../services/transport';

export interface HfDownloadState {
  handleHfDownload: (modelId: string, quantization: string) => Promise<void>;
  downloadsDisabled: boolean;
  disabledReason: string | undefined;
}

/**
 * Queue a HuggingFace model for download from the inspector's preview, and
 * say whether the queue has room for another.
 */
export function useHfDownload(downloadQueue: QueueSnapshot | null | undefined): HfDownloadState {
  const { showToast } = useToastContext();

  const handleHfDownload = useCallback(async (modelId: string, quantization: string) => {
    try {
      await getTransport().queueDownload({ modelId, quantization });
      showToast(`Download queued: ${modelId}`, 'success');
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to start download';
      showToast(message, 'error');
      appLogger.error('component.model', 'Failed to start download', { error });
    }
  }, [showToast]);

  // The daemon says whether it would refuse another; nothing is counted here.
  const downloadsDisabled = downloadQueue?.full ?? false;
  const disabledReason = downloadsDisabled ? 'Download queue is full' : undefined;

  return { handleHfDownload, downloadsDisabled, disabledReason };
}
