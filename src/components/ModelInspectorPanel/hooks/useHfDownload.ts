import { useCallback } from 'react';
import { appLogger } from '../../../services/platform';
import type { DownloadQueueStatus } from '../../../services/transport/types/downloads';
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
export function useHfDownload(queueStatus: DownloadQueueStatus | null | undefined): HfDownloadState {
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

  const maxQueueSize = queueStatus?.max_size ?? 3;
  const currentQueueCount = (queueStatus?.current ? 1 : 0) + (queueStatus?.pending?.length ?? 0);
  const downloadsDisabled = currentQueueCount >= maxQueueSize;
  const disabledReason = downloadsDisabled
    ? `Download queue is full (${currentQueueCount}/${maxQueueSize})`
    : undefined;

  return { handleHfDownload, downloadsDisabled, disabledReason };
}
