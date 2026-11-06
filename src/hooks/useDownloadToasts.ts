import { useEffect } from 'react';
import { useToastContext } from '../contexts/ToastContext';
import { onDownloadEndings } from '../services/downloadsEvents';

/**
 * How downloads ended, as toasts, on whatever screen is open: mounted once,
 * by the app's layout. Completions come batched (`downloadsEvents`): one
 * download is said in the daemon's words, several are counted. Each failure
 * is its own error toast, in the daemon's words.
 */
export function useDownloadToasts(): void {
  const { showToast } = useToastContext();
  useEffect(
    () =>
      onDownloadEndings({
        completed: (items) =>
          showToast(items.length === 1 ? items[0].text : `${items.length} models downloaded`, 'success'),
        failed: (info) => showToast(info.text, 'error'),
      }),
    [showToast],
  );
}
