/**
 * A sentence with a countdown in it, ticking each second for the eye and
 * told once to a screen reader. The reader's copy is the sentence as the
 * countdown to `at` began, so a live region around it announces it once
 * per deadline, never each second; the ticking copy is hidden from it.
 *
 * @module components/status/Countdown
 */

import { useMemo, type FC } from 'react';
import { useSecondsLeft } from '../../hooks/useSecondsLeft';
import { secondsLeft } from '../../utils/countdown';

export const Countdown: FC<{ at: number; say: (seconds: number) => string }> = ({ at, say }) => {
  const left = useSecondsLeft(at) ?? 0;
  const atStart = useMemo(() => secondsLeft(at), [at]);
  return (
    <>
      <span className="sr-only">{say(atStart)}</span>
      <span aria-hidden="true">{say(left)}</span>
    </>
  );
};
