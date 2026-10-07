import { useState, useEffect, useCallback } from 'react';
import { getSetupStatus } from '../services/transport/api/setup';
import { formatError } from '../utils/errors';

/**
 * Whether llama.cpp is installed on the daemon's machine, and whether a
 * prebuilt binary exists for it. Read from the daemon's setup-status route,
 * the one the setup wizard reads, so the desktop app and a browser tab are
 * told the same thing.
 */
export function useLlamaStatus() {
  const [status, setStatus] = useState<{ installed: boolean; canDownload: boolean } | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const checkStatus = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const setup = await getSetupStatus();
      setStatus({ installed: setup.llamaInstalled, canDownload: setup.llamaCanDownload });
    } catch (err) {
      setError(`Failed to check llama status: ${formatError(err)}`);
      // Assume installed if we can't check (fail open)
      setStatus({ installed: true, canDownload: false });
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial status check
  useEffect(() => {
    checkStatus();
  }, [checkStatus]);

  return {
    status,
    loading,
    error,
    checkStatus,
  };
}
