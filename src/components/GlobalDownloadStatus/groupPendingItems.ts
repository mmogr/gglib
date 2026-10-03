import type { DownloadQueueItem } from '../../services/transport/types/downloads';

/**
 * Grouped queue item for display - a model's files are collapsed into one entry
 */
export interface GroupedQueueItem {
  /** Canonical ID string for the group (or single item) */
  id: string;
  /** Human-readable display name */
  display_name: string;
  /** group_id for sharded models, undefined for single items */
  group_id?: string;
  /** Number of weights files of this group in the queue; a projector is not a part */
  shard_count: number;
  /** Position of the first item in this group */
  position: number;
}

/**
 * Groups pending queue items by group_id (a model's download group) or id.
 * A model appears as a single entry, whether it is one file, several shards,
 * or weights with a projector; the count is of its weights files alone.
 */
export function groupPendingItems(items: DownloadQueueItem[]): GroupedQueueItem[] {
  const groups = new Map<string, GroupedQueueItem>();
  
  for (const item of items) {
    // Use group_id for a model's group, id for single items
    const key = item.group_id || item.id;
    const parts = item.shard_info?.role === 'Projector' ? 0 : 1;
    
    const existing = groups.get(key);
    if (!existing) {
      groups.set(key, {
        id: item.id,
        display_name: item.display_name,
        group_id: item.group_id || undefined,
        shard_count: parts,
        position: item.position,
      });
    } else {
      existing.shard_count += parts;
      // Keep the lowest position (first file)
      if (item.position < existing.position) {
        existing.position = item.position;
      }
    }
  }
  
  // Sort by position
  return Array.from(groups.values()).sort((a, b) => a.position - b.position);
}
