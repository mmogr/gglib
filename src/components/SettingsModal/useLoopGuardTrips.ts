/**
 * State for the loop guard's log, read under its own setting.
 *
 * One request, one loading state and a reload, like `useDiagnostics`. It is
 * read when the panel mounts — when the Advanced section opens — rather than
 * with the rest of the settings, because it is a reading, not a setting.
 */

import { useCallback, useEffect, useState } from 'react';
import { getLoopGuardTrips } from '../../services/transport/api/proxy';
import type { LoopGuardTripDay } from '../../types/generated/LoopGuardTripDay';

/**
 * The window the panel reads, in days: the same default the daemon's route and
 * `gglib proxy trips` use (`LOOP_GUARD_LOG_DEFAULT_DAYS` in gglib-core), sent
 * explicitly so the panel's heading and the rows it shows cannot disagree.
 */
export const LOOP_GUARD_LOG_DAYS = 30;

export interface LoopGuardTripsState {
  days: LoopGuardTripDay[] | null;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
}

export function useLoopGuardTrips(): LoopGuardTripsState {
  const [days, setDays] = useState<LoopGuardTripDay[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    setLoading(true);
    setError(null);
    return getLoopGuardTrips(LOOP_GUARD_LOG_DAYS)
      .then(setDays)
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { days, loading, error, reload };
}
