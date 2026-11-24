/**
 * A countdown to a deadline, read the same way wherever one is shown: a
 * pairing code's time left and the wait for Hugging Face's rate limit.
 *
 * @module utils/countdown
 */

/** Whole seconds from `now` until `expiresAt`, never below zero. */
export function secondsLeft(expiresAt: number, now: number = Date.now()): number {
  return Math.max(0, Math.ceil((expiresAt - now) / 1000));
}

/** Seconds as a countdown reads them: `1:42`, `0:09`. */
export function clock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}
