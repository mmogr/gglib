import { useState, useEffect, useCallback } from 'react';
import { getSetupStatus } from '../services/transport/api/setup';
import type { SetupStatus } from '../types/setup';
import { formatError } from '../utils/errors';

/** The update the shell offers: what is installed, and the pinned release. */
export interface LlamaUpdate {
  installed: string;
  target: string;
}

/** What the shell knows of llama.cpp on the daemon's machine. */
export interface LlamaShellStatus {
  installed: boolean;
  canDownload: boolean;
  /**
   * The update to offer, as a missing install is: what is installed is
   * older than the pinned release, the update is a download, and no chat
   * model runs on llama-server. An update the command line would build is
   * the terminal's.
   */
  update: LlamaUpdate | null;
}

/**
 * The update `setup` calls for, if any: one the daemon can run, a download,
 * under no running chat model. One the command line would build is left to
 * it, as the Settings card says.
 */
function updateOf(setup: SetupStatus): LlamaUpdate | null {
  const pin = setup.llamaPin;
  if (pin?.standing !== 'older' || !pin.updateDownloads || setup.llamaServing || !pin.targetRelease) {
    return null;
  }
  return { installed: pin.installedRelease ?? 'unknown', target: pin.targetRelease };
}

/**
 * Whether llama.cpp is installed on the daemon's machine, whether a prebuilt
 * binary exists for it, and the update to offer. Read from the daemon's
 * setup-status route, the one the setup wizard reads, so the desktop app and
 * a browser tab are told the same thing.
 */
export function useLlamaStatus() {
  const [status, setStatus] = useState<LlamaShellStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const checkStatus = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const setup = await getSetupStatus();
      setStatus({
        installed: setup.llamaInstalled,
        canDownload: setup.llamaCanDownload,
        update: updateOf(setup),
      });
    } catch (err) {
      setError(`Failed to check llama status: ${formatError(err)}`);
      // Assume installed if we can't check (fail open)
      setStatus({ installed: true, canDownload: false, update: null });
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
