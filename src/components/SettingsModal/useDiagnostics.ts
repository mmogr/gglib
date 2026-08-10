/**
 * State for the diagnostics half of the System tab.
 *
 * Everything here is read together in one request, so there is one loading
 * state rather than four. The accelerator toggle re-reads afterwards because
 * enabling it changes what the panel reports.
 */

import { useCallback, useEffect, useState } from 'react';
import {
  disableFastDownloads,
  enableFastDownloads,
  getDiagnostics,
} from '../../services/transport/api/setup';
import type { Diagnostics } from '../../types/setup';

export interface DiagnosticsState {
  diagnostics: Diagnostics | null;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;

  /** True while the accelerator is being provisioned or removed. */
  togglingAccelerator: boolean;
  acceleratorError: string | null;
  toggleAccelerator: (enable: boolean) => Promise<void>;
}

export function useDiagnostics(): DiagnosticsState {
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [togglingAccelerator, setTogglingAccelerator] = useState(false);
  const [acceleratorError, setAcceleratorError] = useState<string | null>(null);

  // Returns the promise so callers that must not settle before the panel
  // reflects new state (the accelerator toggle) can await it.
  const reload = useCallback(() => {
    setLoading(true);
    setError(null);
    return getDiagnostics()
      .then(setDiagnostics)
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const toggleAccelerator = useCallback(
    async (enable: boolean) => {
      setTogglingAccelerator(true);
      setAcceleratorError(null);
      try {
        if (enable) {
          await enableFastDownloads();
        } else {
          await disableFastDownloads();
        }
        // Await the refetch before clearing the busy flag: provisioning takes
        // minutes, and clearing early makes the button snap back to "Enable"
        // for a beat, which reads as though the work failed.
        await reload();
      } catch (err) {
        // Provisioning fails for ordinary reasons (no Python), and the
        // message carries the remedy — surface it rather than a generic error.
        setAcceleratorError(err instanceof Error ? err.message : String(err));
      } finally {
        setTogglingAccelerator(false);
      }
    },
    [reload],
  );

  return {
    diagnostics,
    loading,
    error,
    reload,
    togglingAccelerator,
    acceleratorError,
    toggleAccelerator,
  };
}
