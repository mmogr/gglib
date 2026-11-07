/**
 * The seconds a pairing code has left, ticking once a second while there is
 * one. Read from the code's own deadline, never from when the caller
 * mounted, so a page or a line drawn a minute later shows the minute gone.
 *
 * @module places/devices/useSecondsLeft
 */

import { useEffect, useState } from 'react';
import { secondsLeft } from './devices';

/** Seconds until `expiresAt`, or `null` with no code on offer. */
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
