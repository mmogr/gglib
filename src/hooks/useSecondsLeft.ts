/**
 * The seconds left until a deadline (a pairing code's, or the end of a wait
 * for Hugging Face's rate limit), ticking once a second while there is one.
 * Read from the deadline itself, never from when the caller mounted, so a
 * page or a line drawn a minute later shows the minute gone.
 *
 * @module hooks/useSecondsLeft
 */

import { useEffect, useState } from 'react';
import { secondsLeft } from '../utils/countdown';

/** Seconds until `expiresAt`, or `null` with no deadline. */
export function useSecondsLeft(expiresAt: number | null): number | null {
  const [left, setLeft] = useState(() => (expiresAt === null ? null : secondsLeft(expiresAt)));

  useEffect(() => {
    if (expiresAt === null) {
      setLeft(null);
      return;
    }
    setLeft(secondsLeft(expiresAt));
    const tick = window.setInterval(() => setLeft(secondsLeft(expiresAt)), 1000);
    return () => window.clearInterval(tick);
  }, [expiresAt]);

  return left;
}
