/**
 * State for the loop guard's log on Diagnostics › Live.
 *
 * One request, one loading state and a reload, like `useDiagnostics`. It is
 * read when the Live tab opens, which draws the log once it has a day, or
 * a failed read with its Retry.
 */

import { useCallback, useEffect, useState } from 'react';
import { getLoopGuardTrips } from '../../../services/transport/api/proxy';
import type { LoopGuardTripDay } from '../../../types/generated/LoopGuardTripDay';
import { formatError } from '../../../utils/errors';

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

  // The error is cleared when an answer lands, not when the read starts:
  // the Live tab draws the log while it holds days or an error, so a Retry
  // that cleared it first would fold the section away under its button.
  const reload = useCallback(() => {
    setLoading(true);
    return getLoopGuardTrips(LOOP_GUARD_LOG_DAYS)
      .then((read) => {
        setDays(read);
        setError(null);
      })
      .catch((err: unknown) => {
        setError(formatError(err));
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { days, loading, error, reload };
}
