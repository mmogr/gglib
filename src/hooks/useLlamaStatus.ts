import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface LlamaStatus {
  installed: boolean;
  canDownload: boolean;
}

export interface LlamaInstallProgress {
  status: 'started' | 'downloading' | 'completed' | 'error';
  downloaded: number;
  total: number;
  percentage: number;
  message: string;
}

// Check if we're running in Tauri (desktop app)
const isTauriApp = typeof (window as any).__TAURI_INTERNALS__ !== 'undefined' ||
                   typeof (window as any).__TAURI__ !== 'undefined';

export function useLlamaStatus() {
  const [status, setStatus] = useState<LlamaStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installProgress, setInstallProgress] = useState<LlamaInstallProgress | null>(null);

  const checkStatus = useCallback(async () => {
    // Only relevant for Tauri desktop app
    if (!isTauriApp) {
      setStatus({ installed: true, canDownload: false });
      setLoading(false);
      return;
    }

    try {
      setLoading(true);
      setError(null);
      const result = await invoke<{ installed: boolean; can_download: boolean }>('check_llama_status');
      setStatus({
        installed: result.installed,
        canDownload: result.can_download,
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to check llama status: ${errorMessage}`);
      // Assume installed if we can't check (fail open)
      setStatus({ installed: true, canDownload: false });
    } finally {
      setLoading(false);
    }
  }, []);

  const installLlama = useCallback(async () => {
    if (!isTauriApp) {
      return;
    }

    try {
      setInstalling(true);
      setError(null);
      setInstallProgress({
        status: 'started',
        downloaded: 0,
        total: 0,
        percentage: 0,
        message: 'Starting installation...',
      });

      await invoke<string>('install_llama');
      
      // Refresh status after installation
      await checkStatus();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to install llama.cpp: ${errorMessage}`);
      setInstallProgress({
        status: 'error',
        downloaded: 0,
        total: 0,
        percentage: 0,
        message: errorMessage,
      });
    } finally {
      setInstalling(false);
    }
  }, [checkStatus]);

  // Initial status check
  useEffect(() => {
    checkStatus();
  }, [checkStatus]);

  // Listen for installation progress events
  useEffect(() => {
    if (!isTauriApp || !installing) {
      return;
    }

    let unlistenFn: (() => void) | undefined;

    const setupListener = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlistenFn = await listen<LlamaInstallProgress>('llama-install-progress', (event) => {
          const progress = event.payload;
          setInstallProgress(progress);

          if (progress.status === 'completed') {
            setTimeout(() => {
              setInstallProgress(null);
              setInstalling(false);
            }, 1500);
          } else if (progress.status === 'error') {
            setInstalling(false);
          }
        });
      } catch (e) {
        console.error('Failed to setup llama install progress listener:', e);
      }
    };

    setupListener();

    return () => {
      if (unlistenFn) {
        unlistenFn();
      }
    };
  }, [installing]);

  return {
    status,
    loading,
    error,
    installing,
    installProgress,
    checkStatus,
    installLlama,
  };
}
