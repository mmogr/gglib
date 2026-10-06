/**
 * The download queue shows one entry per model: its shards and the projector
 * fetched with them collapse into one, and "N parts" counts weights files.
 * The count is the row's `total_shards`: the queue sends one row per model.
 */

import { describe, it, expect } from 'vitest';

import { groupPendingItems } from '../../../src/components/GlobalDownloadStatus/groupPendingItems';
import type { DownloadQueueItem, ShardInfo } from '../../../src/services/transport/types/downloads';

function file(position: number, role: ShardInfo['role'], shardIndex: number, totalShards: number): DownloadQueueItem {
  return {
    id: 'owner/X-GGUF:Q8_0',
    display_name: 'X-GGUF Q8_0',
    status: 'queued',
    position,
    group_id: 'owner/X-GGUF:Q8_0',
    shard_info: { shard_index: shardIndex, total_shards: totalShards, filename: `f${shardIndex}.gguf`, role },
  } as DownloadQueueItem;
}

describe('groupPendingItems', () => {
  it('parts_come_from_total_shards', () => {
    const grouped = groupPendingItems([file(2, 'Weights', 0, 3)]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0].shard_count).toBe(3);
  });

  it('counts a model with no shard info as one part', () => {
    const single: DownloadQueueItem = { ...file(2, 'Weights', 0, 1), group_id: null, shard_info: null };

    expect(groupPendingItems([single])[0].shard_count).toBe(1);
  });

  it('shows a model with a projector as one entry that is not "2 parts"', () => {
    const grouped = groupPendingItems([file(2, 'Weights', 0, 1), file(3, 'Projector', 1, 1)]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0].shard_count).toBe(1);
    expect(grouped[0].position).toBe(2);
  });

  it('counts the shards of a sharded model and leaves its projector out', () => {
    const grouped = groupPendingItems([
      file(4, 'Projector', 2, 2),
      file(2, 'Weights', 0, 2),
      file(3, 'Weights', 1, 2),
    ]);

    expect(grouped).toHaveLength(1);
    expect(grouped[0].shard_count).toBe(2);
    expect(grouped[0].position).toBe(2);
  });

  it('keeps two models apart, in queue order', () => {
    const other: DownloadQueueItem = { ...file(1, 'Weights', 0, 1), id: 'owner/Y:Q4_K_M', group_id: null, shard_info: null };

    const grouped = groupPendingItems([file(2, 'Weights', 0, 1), other]);

    expect(grouped.map((g) => g.id)).toEqual(['owner/Y:Q4_K_M', 'owner/X-GGUF:Q8_0']);
  });
});
