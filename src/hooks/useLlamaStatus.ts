import { useState, useEffect, useCallback } from 'react';
import { 
  checkLlamaInstalled, 
  installLlama as platformInstallLlama, 
  listenLlamaProgress,
  type LlamaStatus,
  type LlamaInstallProgress
} from '../services/platform';

// Re-export types for consumers
export type { LlamaStatus, LlamaInstallProgress };

export function useLlamaStatus() {
  const [status, setStatus] = useState<LlamaStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installProgress, setInstallProgress] = useState<LlamaInstallProgress | null>(null);

  const checkStatus = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await checkLlamaInstalled();
      setStatus(result);
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

      await platformInstallLlama();
      
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
    if (!installing) {
      return;
    }

    let cleanup: (() => void) | null = null;

    listenLlamaProgress((progress) => {
      setInstallProgress(progress);

      if (progress.status === 'completed') {
        setTimeout(() => {
          setInstallProgress(null);
          setInstalling(false);
        }, 1500);
      } else if (progress.status === 'error') {
        setInstalling(false);
      }
    }).then(unsubscribe => {
      cleanup = unsubscribe;
    }).catch(e => {
      console.error('Failed to setup llama install progress listener:', e);
    });

    return () => {
      cleanup?.();
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
