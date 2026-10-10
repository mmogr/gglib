import { useEffect, useState } from 'react';
import { getSdStatus } from '../../../services/transport/api/setup';
import type { ImageRuntimeStatus } from '../../../types/setup';
import { formatError } from '../../../utils/errors';

export interface SdStatusState {
  /** `null` while it is read, and when it is not asked for. */
  status: ImageRuntimeStatus | null;
  error: string | null;
}

/**
 * The image runtime's status, read once when `enabled`: the serve modal asks
 * it for an image model only, to say whether `sd-server` is there to load it.
 */
export function useSdStatus(enabled: boolean): SdStatusState {
  const [state, setState] = useState<SdStatusState>({ status: null, error: null });

  useEffect(() => {
    if (!enabled) return;
    let live = true;
    getSdStatus()
      .then((status) => live && setState({ status, error: null }))
      .catch((err: unknown) => live && setState({ status: null, error: formatError(err) }));
    return () => {
      live = false;
    };
  }, [enabled]);

  return state;
}
