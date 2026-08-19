/**
 * How a flat queue snapshot becomes `{current, pending, failed}`.
 *
 * Two paths produce that shape — the SSE `queue_snapshot` frame and
 * `GET /api/models/downloads/queue` — and they used to hold separate copies of
 * the rules. The copies agreed, so this is not a regression test for a bug; it
 * pins the rules now that there is one implementation to pin.
 *
 * **Most of the statuses below cannot reach these functions.** A snapshot
 * carries `downloading` on the single active row and `queued` on the rest —
 * `gglib-download` hard-codes both — and failures live in a `recent_failures`
 * list the wire type does not carry. The `finalizing`, `registering`,
 * `completed`, `failed` and `cancelled` cases are here because
 * `DownloadStatus` admits them and the predicates answer for them: they say
 * what the code would do, not what the server does.
 */

import { describe, it, expect } from 'vitest';

import { bucketQueue, isInFlight } from '../../../src/services/transport/downloadQueue';
import type { DownloadQueueItem } from '../../../src/services/transport/types/downloads';
import type { DownloadStatus } from '../../../src/types/generated/DownloadStatus';

function item(status: DownloadStatus, id: string = status): DownloadQueueItem {
  return { id, display_name: id, status, position: 0 };
}

describe('isInFlight', () => {
  it('counts the status a snapshot actually carries', () => {
    expect(isInFlight(item('downloading'))).toBe(true);
  });

  // Unreachable today. Named so that if the queue ever reports its tail
  // phases, the answer is already decided and visible rather than falling out
  // of whichever predicate happens not to match.
  it.each(['finalizing', 'registering'] as const)(
    'would count %s as work in progress, not as nothing',
    (status) => {
      expect(isInFlight(item(status))).toBe(true);
    },
  );

  it.each(['queued', 'completed', 'failed', 'cancelled'] as const)(
    'does not count %s',
    (status) => {
      expect(isInFlight(item(status))).toBe(false);
    },
  );
});

describe('bucketQueue', () => {
  it('sorts the two statuses a snapshot carries', () => {
    const { current, pending } = bucketQueue([
      item('queued', 'a'),
      item('downloading', 'b'),
      item('queued', 'c'),
    ]);

    expect(current?.id).toBe('b');
    expect(pending.map((i) => i.id)).toEqual(['a', 'c']);
  });

  it('reports no current download when nothing is in flight', () => {
    expect(bucketQueue([item('queued')]).current).toBeNull();
  });

  /**
   * `failed` is always empty: the queue keeps failures in `recent_failures`,
   * which the snapshot does not carry, and nothing renders the bucket. This
   * pins that a row which did somehow arrive `failed` is not mistaken for
   * pending or current.
   */
  it('keeps a failed row out of the buckets that drive the UI', () => {
    const { current, pending, failed } = bucketQueue([item('failed')]);

    expect(current).toBeNull();
    expect(pending).toEqual([]);
    expect(failed).toHaveLength(1);
  });

  it('does not report a cancelled download as failed', () => {
    expect(bucketQueue([item('cancelled')]).failed).toEqual([]);
  });
});
