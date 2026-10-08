/**
 * The download queue as the daemon serves it.
 *
 * Shapes and words are the ones `gglib_core::download::row` makes, read from
 * `crates/gglib-core/src/download/row.rs` and its tests: every `Option` there
 * skips when it is `None`, so an unknown value is an absent key, never a
 * `null`; and a field of `text` with nothing to say is an empty string.
 */

import type { DownloadRow, QueueSnapshot } from '../../../src/services/transport/types/downloads';

const GIB = 1024 ** 3;

/**
 * The running download, a quarter of the way through one file of 28 GiB.
 *
 * `text.file` is absent, as it is for a download of one file.
 */
export function runningRow(overrides: Partial<DownloadRow> = {}): DownloadRow {
  return {
    id: 'owner/zeta-GGUF:Q8_0',
    model_id: 'owner/zeta-GGUF',
    quantization: 'Q8_0',
    phase: 'downloading',
    position: 1,
    downloaded_bytes: 7 * GIB,
    total_bytes: 28 * GIB,
    percent: 25,
    speed_bps: 118_400_000,
    eta_seconds: 160,
    text: {
      title: 'owner/zeta-GGUF:Q8_0',
      status: 'Downloading',
      bytes: '7.00 GiB / 28.00 GiB',
      percent: '25.0%',
      speed: '118.4 MB/s',
      eta: 'ETA 2m 40s',
    },
    ...overrides,
  };
}

/**
 * A download that has not started. Its size is known, so its percentage is a
 * number, 0, while the text for it is empty; it has no speed and no time
 * remaining.
 */
export function waitingRow(id: string, position: number, overrides: Partial<DownloadRow> = {}): DownloadRow {
  const [model_id, quantization] = id.split(':');
  return {
    id,
    model_id,
    ...(quantization ? { quantization } : {}),
    phase: 'queued',
    position,
    downloaded_bytes: 0,
    total_bytes: 5 * GIB,
    percent: 0,
    text: { title: id, status: 'Queued', bytes: '5.00 GiB', percent: '', speed: '', eta: '' },
    ...overrides,
  };
}

/**
 * A snapshot. With no overrides it is the idle queue: no `active` key, nothing
 * waiting, nothing finished, and the default capacity of 10.
 */
export function queueSnapshot(overrides: Partial<QueueSnapshot> = {}): QueueSnapshot {
  return { revision: 1, waiting: [], finished: [], max_size: 10, full: false, ...overrides };
}
