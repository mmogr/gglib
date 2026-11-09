/**
 * The download accelerator in setup: set up, without a question, when Get
 * it is pressed, as `gglib up` sets it up once a download is agreed
 * (`up/choose.rs`), and not before: a Python environment is not built for
 * a setup that then skips the model or joins another machine. Where
 * Python 3 is not there it is never tried. Downloads work without it, so
 * its failure is a warning with Retry, and the download goes ahead.
 *
 * @module places/setup/useFastDownloads
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { enableFastDownloads } from '../../services/transport/api/setup';
import { formatError } from '../../utils/errors';
import type { FastDownloadsState } from '../../types/generated/FastDownloadsState';

/** Ready; never tried (no Python 3); waiting for Get it; being set up; or why it failed. */
export type FastState = 'ready' | 'no-python' | 'later' | 'setting-up' | { failed: string };

export interface FastDownloads {
  fast: FastState;
  /** Set it up now, if it is to be and is not already; settles however it ends. */
  setUp: () => Promise<void>;
  /** Try again after a failure. */
  retry: () => void;
}

/**
 * `served` and `python` are the setup status's: the accelerator's state
 * (`fastDownloads`, On once it is set up), and whether it can be set up.
 */
export function useFastDownloads(served: FastDownloadsState, python: boolean): FastDownloads {
  const [fast, setFast] = useState<FastState>(served === 'on' ? 'ready' : python ? 'later' : 'no-python');
  const running = useRef<Promise<void> | null>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const run = useCallback(() => {
    if (running.current) return running.current;
    setFast('setting-up');
    running.current = enableFastDownloads()
      .then(() => {
        if (live.current) setFast('ready');
      })
      .catch((err: unknown) => {
        if (live.current) setFast({ failed: formatError(err) });
      })
      .finally(() => {
        running.current = null;
      });
    return running.current;
  }, []);

  const setUp = useCallback(() => (fast === 'later' ? run() : (running.current ?? Promise.resolve())), [fast, run]);

  return { fast, setUp, retry: () => void run() };
}
