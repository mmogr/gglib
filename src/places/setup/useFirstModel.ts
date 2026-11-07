/**
 * Setup's step 3, as state the place holds: what was chosen for a first
 * model, and the download Get it queued, followed in `downloadsRegistry`
 * by the id the queue answered with, until the library has the model.
 *
 * The model is found as `gglib up` finds it (`find_downloaded`): the
 * library's model whose Hugging Face repository is the one queued, the
 * newest if there are two. Its row's words are the daemon's (TX-13); a
 * download that ended without a model is said by its finished entry's text.
 *
 * A queue that refuses the repo because it is already queued (a reload
 * mid-download, or a queue whose read after the call failed) has its row
 * followed instead: the queue is read again and the repo's row taken, so
 * the step shows the download that runs, not a Retry that can only fail.
 * Any other refusal is said as it came.
 *
 * @module places/setup/useFirstModel
 */

import { useCallback, useState, useRef } from 'react';
import { useDownloads } from '../../services/downloadsRegistry';
import { queueDownload } from '../../services/downloadsEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import type { DownloadRow, FinishedDownload, QueueSnapshot } from '../../services/transport/types/downloads';
import type { GgufModel } from '../../types';
import type { RecommendationDto } from '../../types/setup';
import { formatError } from '../../utils/errors';

/**
 * What step 3 came to: the recommendation downloading, skipped, another
 * model to be chosen in the Library once setup is done, or another
 * machine's models joined.
 */
export type FirstChoice = 'get' | 'skip' | 'another' | 'joined';

export interface FirstModel {
  choice: FirstChoice | null;
  choose: (choice: Exclude<FirstChoice, 'get'>) => void;
  /** Queue the recommendation; again after a failure. */
  get: () => Promise<void>;
  /** From Get it until the queue has answered, the accelerator's set-up first. */
  queueing: boolean;
  /** Why the queue refused it, as the daemon said. */
  queueError: string | null;
  /** The download's row while it is queued or running. */
  row: DownloadRow | null;
  /** How it ended, once it has. */
  finished: FinishedDownload | null;
  /** The library's model it became. */
  model: GgufModel | null;
}

/** The queue's rows, the running one first. */
const rowsOf = (snapshot: QueueSnapshot | null): DownloadRow[] =>
  snapshot ? [...(snapshot.active ? [snapshot.active] : []), ...snapshot.waiting] : [];

/** The library's newest chat model from `repo`. */
export function modelFrom(models: GgufModel[], repo: string): GgufModel | null {
  const from = models.filter((m) => m.hfRepoId === repo && m.imageFamily === undefined);
  return from.sort((a, b) => b.id - a.id)[0] ?? null;
}

/** `prepare` runs first on Get it, however it ends: the accelerator's set-up. */
export function useFirstModel(rec: RecommendationDto | null, prepare: () => Promise<void>): FirstModel {
  const [choice, setChoice] = useState<FirstChoice | null>(null);
  const [downloadId, setDownloadId] = useState<string | null>(null);
  const [queueing, setQueueing] = useState(false);
  const [queueError, setQueueError] = useState<string | null>(null);
  const { snapshot } = useDownloads();
  const { models } = useModelLibrary();

  // Which Get it is current: a choice made while its set-up runs (Skip,
  // Choose another, Join) cancels it, so the model is not queued after all.
  const asked = useRef(0);
  const choose = useCallback((next: FirstChoice | null) => {
    asked.current += 1;
    setQueueing(false);
    setChoice(next);
  }, []);

  const get = useCallback(async () => {
    if (rec === null) return;
    const mine = ++asked.current;
    setQueueing(true);
    setQueueError(null);
    await prepare();
    if (asked.current !== mine) return;
    try {
      // The queue attaches a download already under way and answers its id,
      // so a second Get it follows the first.
      setDownloadId(await queueDownload(rec.repo));
      if (asked.current === mine) setChoice('get');
    } catch (err) {
      if (asked.current === mine) setQueueError(formatError(err));
    } finally {
      if (asked.current === mine) setQueueing(false);
    }
  }, [rec, prepare]);

  const rows = rowsOf(snapshot);
  const row = downloadId === null ? null : (rows.find((r) => r.id === downloadId) ?? null);
  // A download queued again keeps its last ending until it ends again: its row says it is back.
  const ended = downloadId === null || row !== null ? null : (snapshot?.finished.find((f) => f.id === downloadId) ?? null);
  const model = choice === 'get' && rec !== null ? modelFrom(models, rec.repo) : null;

  return { choice, choose, get, queueing, queueError, row, finished: ended, model };
}
