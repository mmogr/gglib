/**
 * The Load palette's fit reads: `GET /api/models/:id/explain` per row,
 * lazily, since there is no route that explains many models at once.
 *
 * `wanted` is the order to read in (the highlighted row first, then the rows
 * in view); at most three reads are in flight, the next starting as one
 * lands, counting reads an earlier open left still out (a quick close and
 * reopen does not put six on the daemon). An answer is kept for this open of the palette, so a row read once
 * is not read again when it is highlighted again. `begin` starts an open
 * with nothing read and `end` closes it; an answer to an earlier open, or
 * one landing after the close, is dropped (the open's number, as the
 * library's search drops an answer to an earlier Enter). A failed read, or
 * a daemon without the route (`null`), is `'failed'`, said as "fit
 * unknown", never a wait without end.
 *
 * @module places/endpoint/useFitReads
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { explainModel } from '../../services/transport/api/models/local';
import type { FitRead } from './paletteRows';

/** Reads in flight at once, at most. */
const MAX_READS = 3;

const READING = 'reading';

export interface FitReads {
  /** Each row's read once it has landed, by model id. */
  reads: ReadonlyMap<number, FitRead>;
  /** An open starts: nothing read yet. */
  begin: () => void;
  /** The palette closed: answers still to come are dropped. */
  end: () => void;
}

export function useFitReads(wanted: readonly number[]): FitReads {
  const opened = useRef(0);
  const live = useRef(false);
  const asked = useRef(new Map<number, FitRead | typeof READING>());
  // Reads out now, whichever open asked for them.
  const out = useRef(0);
  const [reads, setReads] = useState<ReadonlyMap<number, FitRead>>(() => new Map());
  // Counts every read that settles, kept or dropped: each may let the next start.
  const [settled, setSettled] = useState(0);

  const begin = useCallback(() => {
    opened.current += 1;
    live.current = true;
    asked.current = new Map();
    setReads(new Map());
  }, []);
  const end = useCallback(() => {
    opened.current += 1;
    live.current = false;
  }, []);

  const order = wanted.join(',');
  // Runs again whenever a read settles, to start the next.
  useEffect(() => {
    if (!live.current || order === '') return;
    const open = opened.current;
    const mine = asked.current;
    for (const id of order.split(',').map(Number)) {
      if (out.current >= MAX_READS) break;
      if (mine.has(id)) continue;
      mine.set(id, READING);
      out.current += 1;
      void explainModel(id)
        .then(
          (read): FitRead => read ?? 'failed',
          (): FitRead => 'failed',
        )
        .then((read) => {
          out.current -= 1;
          setSettled((n) => n + 1);
          if (open !== opened.current) return;
          mine.set(id, read);
          setReads((now) => new Map(now).set(id, read));
        });
    }
  }, [order, reads, settled]);

  return { reads, begin, end };
}
