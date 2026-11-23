/**
 * Queue a download from a Download button, and say what came of it in a
 * toast: queued, held until Hugging Face's rate limit lifts (its row is in
 * the library's Downloading group, and it is queued then by itself), or why
 * it was refused, in the daemon's words.
 *
 * @module hooks/useQueueDownload
 */

import { useCallback } from 'react';
import { useToastContext } from '../contexts/ToastContext';
import { queueDownload } from '../services/downloadsEvents';
import { formatError } from '../utils/errors';

/** Answers whether the download was queued or held. */
export function useQueueDownload(): (repo: string, quantization?: string) => Promise<boolean> {
  const { showToast } = useToastContext();
  return useCallback(
    async (repo, quantization) => {
      try {
        const queued = await queueDownload(repo, quantization);
        if (queued.held) showToast(`${repo} will be queued when Hugging Face's rate limit lifts`, 'info');
        else showToast(`Download queued: ${repo}`, 'success');
        return true;
      } catch (err) {
        showToast(formatError(err), 'error');
        return false;
      }
    },
    [showToast],
  );
}
