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
 * @module places/setup/useFirstModel
 */

import { useCallback, useState } from 'react';
import { useDownloads } from '../../services/downloadsRegistry';
import { queueDownload } from '../../services/downloadsEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import type { DownloadRow, FinishedDownload } from '../../services/transport/types/downloads';
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

/** The library's newest chat model from `repo`. */
export function modelFrom(models: GgufModel[], repo: string): GgufModel | null {
  const from = models.filter((m) => m.hfRepoId === repo && m.imageFamily === undefined);
  return from.sort((a, b) => b.id - a.id)[0] ?? null;
}

export function useFirstModel(rec: RecommendationDto | null): FirstModel {
  const [choice, setChoice] = useState<FirstChoice | null>(null);
  const [downloadId, setDownloadId] = useState<string | null>(null);
  const [queueing, setQueueing] = useState(false);
  const [queueError, setQueueError] = useState<string | null>(null);
  const { snapshot } = useDownloads();
  const { models } = useModelLibrary();

  const get = useCallback(async () => {
    if (rec === null) return;
    setQueueing(true);
    setQueueError(null);
    try {
      setDownloadId(await queueDownload(rec.repo));
      setChoice('get');
    } catch (err) {
      setQueueError(formatError(err));
    } finally {
      setQueueing(false);
    }
  }, [rec]);

  const rows = snapshot ? [...(snapshot.active ? [snapshot.active] : []), ...snapshot.waiting] : [];
  const row = downloadId === null ? null : (rows.find((r) => r.id === downloadId) ?? null);
  // A download queued again keeps its last ending until it ends again: its row says it is back.
  const ended = downloadId === null || row !== null ? null : (snapshot?.finished.find((f) => f.id === downloadId) ?? null);
  const model = choice === 'get' && rec !== null ? modelFrom(models, rec.repo) : null;

  return { choice, choose: setChoice, get, queueing, queueError, row, finished: ended, model };
}
