/**
 * How a flat queue snapshot becomes `{current, pending, failed}`.
 *
 * The wire reports seven statuses. Two of them — `finalizing` and
 * `registering` — are the tail of a download: the bytes are on disk and the
 * queue is checksumming and writing the row. Both sides of the GUI used to
 * treat them as *nothing*: they matched neither `current` nor `pending`, so
 * the download card unmounted for the duration and came back when the next
 * event arrived, which is the phase whose "Finalizing" and "Registering"
 * labels were written to be shown.
 *
 * That it stayed on screen at all was a race — a `download_status_changed`
 * event re-setting the active id before React committed the unmount.
 */

import { describe, it, expect } from 'vitest';

import { bucketQueue, isInFlight } from '../../../src/services/transport/downloadQueue';
import type { DownloadQueueItem } from '../../../src/services/transport/types/downloads';
import type { DownloadStatus } from '../../../src/types/generated/DownloadStatus';

function item(status: DownloadStatus, id: string = status): DownloadQueueItem {
  return { id, display_name: id, status, position: 0 };
}

describe('isInFlight', () => {
  it.each(['downloading', 'finalizing', 'registering'] as const)(
    'counts %s as work in progress',
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
  it('keeps a finalizing row as the current download', () => {
    const { current } = bucketQueue([item('finalizing')]);

    expect(current?.status).toBe('finalizing');
  });

  it('keeps a registering row as the current download', () => {
    const { current } = bucketQueue([item('registering')]);

    expect(current?.status).toBe('registering');
  });

  it('sorts the ordinary statuses into their buckets', () => {
    const { current, pending, failed } = bucketQueue([
      item('queued', 'a'),
      item('downloading', 'b'),
      item('queued', 'c'),
      item('failed', 'd'),
      item('completed', 'e'),
    ]);

    expect(current?.id).toBe('b');
    expect(pending.map((i) => i.id)).toEqual(['a', 'c']);
    expect(failed.map((i) => i.id)).toEqual(['d']);
  });

  /**
   * `cancelled` is its own status on the wire and is not folded into `failed`.
   * The SSE path used to map it across, which put a row the user cancelled on
   * a list of things that went wrong — a list nothing renders, so the fold
   * bought nothing and lost the distinction.
   */
  it('does not report a cancelled download as failed', () => {
    const { failed } = bucketQueue([item('cancelled')]);

    expect(failed).toEqual([]);
  });

  it('reports no current download when nothing is in flight', () => {
    expect(bucketQueue([item('queued'), item('completed')]).current).toBeNull();
  });
});
