/**
 * The page's one answer to Hugging Face's rate limit: wait until the daemon
 * says it lifts, then ask again. Every Hub read and queue request the page
 * makes on its own goes through `whenHubAllows`; none times a wait itself.
 *
 * @module services/transport/hubLimit
 */

import { hubLimitOf } from './errors';

/** Limited answers in a row after which the last one is thrown. */
const MOST_LIMITED_ANSWERS = 5;

/** How the wait is stopped and told. */
export interface HubWaitOptions {
  /** Ends a wait at once, and stops the next ask; the promise rejects with an `AbortError`. */
  signal?: AbortSignal;
  /** Told when the limit lifts as a wait starts (epoch ms), and `null` as the ask goes out again. */
  onWaiting?: (liftsAt: number | null) => void;
}

function aborted(): DOMException {
  return new DOMException('The wait for the Hub was stopped.', 'AbortError');
}

/** Resolve at `at` (epoch ms), or reject as `signal` aborts. */
function waitUntil(at: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(aborted());
      return;
    }
    const stop = () => {
      clearTimeout(timer);
      reject(aborted());
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', stop);
      resolve();
    }, Math.max(0, at - Date.now()));
    signal?.addEventListener('abort', stop, { once: true });
  });
}

/**
 * Run `action`; while the daemon refuses it for the Hub's limit, wait until
 * a second after the limit lifts and run it again. The fifth limited answer
 * in a row is thrown, and any other error is thrown as it came.
 */
export async function whenHubAllows<T>(action: (signal?: AbortSignal) => Promise<T>, { signal, onWaiting }: HubWaitOptions = {}): Promise<T> {
  for (let answers = 1; ; answers++) {
    if (signal?.aborted) throw aborted();
    try {
      return await action(signal);
    } catch (err) {
      const limit = hubLimitOf(err);
      if (limit === null || answers >= MOST_LIMITED_ANSWERS) throw err;
      const liftsAt = Date.now() + (limit.retryAfterSecs + 1) * 1000;
      onWaiting?.(liftsAt);
      await waitUntil(liftsAt, signal);
      onWaiting?.(null);
    }
  }
}
