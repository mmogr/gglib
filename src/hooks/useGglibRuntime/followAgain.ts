/**
 * When a run's stream that ended before the run did is read again.
 *
 * A stream can end early while its run goes on: a proxy that hiccups, a
 * service that restarts its listener, a machine that slept. Each such end is
 * followed by another try from the last event read, after a wait that grows
 * (`WAITS_MS`), until the tries are spent. A try that reads a new event starts
 * the waits again, so a long reply that drops now and then is never given up
 * on. A stream that ends with the run's end is never tried again, and neither
 * is one the service refused (a run it no longer has): only a stream that
 * closed, failed to connect, broke, timed out before its headers, or was
 * refused for a stale token (renewed first) or by a failing service.
 *
 * Each try's request is limited as every stream is (`timeLimit.ts`); the only
 * timer here is the wait between tries, which leaving clears at once.
 *
 * @module followAgain
 */

import { TransportError } from '../../services/transport/errors';
import { renewAfterRefusal } from '../../services/transport/api/renew';

/** The wait before each try, in order: as many tries as waits. */
export const WAITS_MS: readonly number[] = [250, 500, 1000, 2000, 4000, 4000, 4000, 4000];

/** Why the last stream ended, when it closed with no error. */
const CLOSED = "gglib's service closed the stream.";

/** Whether a stream that ended with `error` (`null`: it closed) may be tried again. */
function mayTryAgain(error: Error | null): boolean {
  if (!TransportError.isTransportError(error)) return true;
  if (error.code === 'TIMEOUT') return true;
  const status = (error.details as { status?: number } | undefined)?.status;
  return status === 401 || (status !== undefined && status >= 500);
}

/** Wait `ms`, or less when `signal` fires first; true when it did not. */
function waitUnlessLeft(ms: number, signal: AbortSignal): Promise<boolean> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve(false);
    const left = () => {
      clearTimeout(timer);
      resolve(false);
    };
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', left);
      resolve(true);
    }, ms);
    signal.addEventListener('abort', left, { once: true });
  });
}

/** One reading's tries: what `drawRun` asks when a stream ends early, and whether they were spent. */
export interface FollowAgain {
  /**
   * The stream ended before the run, with `error` or none; `progressed` when
   * it read a new event. Resolves true to read again, after the wait.
   */
  again(error: Error | null, progressed: boolean): Promise<boolean>;
  /** The sentence the page shows once the tries are spent; null until then. */
  spent(): string | null;
}

/** The tries of the reading `signal` belongs to. */
export function followAgain(signal: AbortSignal): FollowAgain {
  let tries = 0;
  let spent: string | null = null;
  return {
    async again(error, progressed) {
      if (signal.aborted || !mayTryAgain(error)) return false;
      if (progressed) tries = 0;
      if (tries >= WAITS_MS.length) {
        const why = error?.message.trim() || CLOSED;
        spent =
          `The reply stopped arriving and could not be read again: ${why.replace(/([^.])$/, '$1.')} ` +
          'It may still be running; open this conversation again to see it.';
        return false;
      }
      if (!(await waitUnlessLeft(WAITS_MS[tries++], signal))) return false;
      await renewAfterRefusal(error).catch(() => {}); // a restarted service minted a new token
      return !signal.aborted;
    },
    spent: () => spent,
  };
}
