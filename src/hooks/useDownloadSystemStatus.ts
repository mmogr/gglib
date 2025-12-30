import { useEffect, useState } from 'react';
import { isDesktop } from '../services/platform';

export type DownloadSystemStatus =
  | { status: 'initializing' }
  | { status: 'ready' }
  | { status: 'error'; message: string };

const DOWNLOAD_SYSTEM_READY_EVENT = 'download-system:ready';
const DOWNLOAD_SYSTEM_ERROR_EVENT = 'download-system:error';

type DownloadSystemErrorPayload = {
  message: string;
};

/**
 * Tracks whether the backend download subsystem (Python fast helper) is usable.
 *
 * On desktop (Tauri), the backend emits explicit init events on app startup.
 * On web, this is always ready (fast helper not applicable).
 */
export function useDownloadSystemStatus(): DownloadSystemStatus {
  const [state, setState] = useState<DownloadSystemStatus>({
    status: isDesktop() ? 'initializing' : 'ready',
  });

  useEffect(() => {
    if (!isDesktop()) {
      return;
    }

    let cancelled = false;
    let unlistenReady: null | (() => void) = null;
    let unlistenError: null | (() => void) = null;

    (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');

        unlistenReady = await listen<boolean>(DOWNLOAD_SYSTEM_READY_EVENT, () => {
          if (cancelled) return;
          setState({ status: 'ready' });
        });

        unlistenError = await listen<DownloadSystemErrorPayload>(DOWNLOAD_SYSTEM_ERROR_EVENT, (event) => {
          if (cancelled) return;
          const message = event.payload?.message || 'Download system initialization failed';
          setState({ status: 'error', message });
        });
      } catch (e) {
        if (cancelled) return;
        const message = e instanceof Error ? e.message : String(e);
        setState({ status: 'error', message: `Failed to subscribe to download init events: ${message}` });
      }
    })();

    return () => {
      cancelled = true;
      try {
        unlistenReady?.();
      } catch {
        // ignore
      }
      try {
        unlistenError?.();
      } catch {
        // ignore
      }
    };
  }, []);

  return state;
}
