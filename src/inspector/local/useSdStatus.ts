import { useEffect, useState } from 'react';
import { useInstalledVersion } from '../../hooks/useEngine';
import { getEngineStatus } from '../../services/transport/api/setup';
import type { EngineStatus } from '../../types/setup';
import { formatError } from '../../utils/errors';

export interface SdStatusState {
  /** `null` while it is read, and when it is not asked for. */
  status: EngineStatus | null;
  error: string | null;
}

/**
 * The image runtime's status, read when `enabled` and again when an install
 * completes (the Load with options sheet's, or System settings'): the
 * inspector asks it for an image model only, to say whether `sd-server` is
 * there to load it.
 */
export function useSdStatus(enabled: boolean): SdStatusState {
  const [state, setState] = useState<SdStatusState>({ status: null, error: null });
  const installed = useInstalledVersion('stable_diffusion');

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    getEngineStatus('stable_diffusion')
      .then((status) => live && setState({ status, error: null }))
      .catch((err: unknown) => live && setState({ status: null, error: formatError(err) }));
    return () => {
      live = false;
    };
  }, [enabled, installed]);

  return state;
}
