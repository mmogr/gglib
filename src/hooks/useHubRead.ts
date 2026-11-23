/**
 * One read of the Hub through the daemon, made when the caller mounts. A
 * read Hugging Face rate limits waits until the limit lifts and is made
 * again (`whenHubAllows`), and while it waits `liftsAt` says until when.
 * Leaving stops the read or the wait still out (`signal`): a Hub read can
 * take minutes, and each one out holds one of the page's six connections to
 * the daemon.
 *
 * @module hooks/useHubRead
 */

import { useEffect, useState } from 'react';
import { whenHubAllows } from '../services/transport/hubLimit';

export interface HubRead<T> {
  /** The answer, or what `onError` made of a failure; `null` until either. */
  answer: T | null;
  /** When the Hub's limit lifts (epoch ms) while the read waits for it, else `null`. */
  liftsAt: number | null;
}

interface HubReadOptions<T> {
  /** Why it failed, as an answer; `null` keeps none. */
  onError: (err: unknown) => T | null;
  /** The answer already in hand, so nothing is read. */
  seed?: T | null;
}

export function useHubRead<T>(read: (signal?: AbortSignal) => Promise<T>, { onError, seed = null }: HubReadOptions<T>): HubRead<T> {
  const [answer, setAnswer] = useState<T | null>(seed);
  const [liftsAt, setLiftsAt] = useState<number | null>(null);
  useEffect(() => {
    if (seed !== null) return;
    let live = true;
    const gone = new AbortController();
    whenHubAllows(read, { signal: gone.signal, onWaiting: (at) => live && setLiftsAt(at) })
      .then((value) => live && setAnswer(value))
      .catch((err: unknown) => {
        if (!live) return;
        setLiftsAt(null);
        setAnswer(onError(err));
      });
    return () => {
      live = false;
      gone.abort();
    };
    // The caller is keyed by what it reads, so this is read once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return { answer, liftsAt };
}
