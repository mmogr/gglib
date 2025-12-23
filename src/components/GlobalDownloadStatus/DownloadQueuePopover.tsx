import { FC, useRef, useState, useCallback, useMemo } from 'react';
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useClickOutside } from '../../hooks/useClickOutside';
import {
  cancelShardGroup,
  removeFromQueue,
  reorderQueue,
} from '../../services/clients/downloads';
import type { DownloadQueueItem } from '../../services/transport/types/downloads';
import { Icon } from '../ui/Icon';
import styles from './DownloadQueuePopover.module.css';

/**
 * Grouped queue item for display - sharded downloads are collapsed into one entry
 */
interface GroupedQueueItem {
  /** Canonical ID string for the group (or single item) */
  id: string;
  /** Human-readable display name */
  display_name: string;
  /** group_id for sharded models, undefined for single items */
  group_id?: string;
  /** Number of shards in this group (1 for non-sharded) */
  shard_count: number;
  /** Position of the first item in this group */
  position: number;
}

interface DownloadQueuePopoverProps {
  /** Whether the popover is open */
  isOpen: boolean;
  /** Called to close the popover */
  onClose: () => void;
  /** Pending items from queue status */
  pendingItems: DownloadQueueItem[];
  /** Called after an item is removed/reordered to refresh queue */
  onRefresh?: () => void;
}

/**
 * Groups pending queue items by group_id (for sharded models) or model_id.
 * Sharded downloads appear as a single entry with shard count indicator.
 */
function groupPendingItems(items: DownloadQueueItem[]): GroupedQueueItem[] {
  const groups = new Map<string, GroupedQueueItem>();
  
  for (const item of items) {
    // Use group_id for sharded, id for single items
    const key = item.group_id || item.id;
    
    if (!groups.has(key)) {
      groups.set(key, {
        id: item.id,
        display_name: item.display_name,
        group_id: item.group_id || undefined,
        shard_count: 1,
        position: item.position,
      });
    } else {
      // Increment shard count for existing group
      const existing = groups.get(key)!;
      existing.shard_count += 1;
      // Keep the lowest position (first shard)
      if (item.position < existing.position) {
        existing.position = item.position;
      }
    }
  }
  
  // Sort by position
  return Array.from(groups.values()).sort((a, b) => a.position - b.position);
}

/**
 * Popover component showing queued downloads with reorder and cancel functionality.
 * Uses up/down buttons for reordering (works in both Tauri WebKit and web browsers).
 * Sharded models are grouped and displayed as a single entry.
 */
const DownloadQueuePopover: FC<DownloadQueuePopoverProps> = ({
  isOpen,
  onClose,
  pendingItems,
  onRefresh,
}) => {
  const popoverRef = useRef<HTMLDivElement>(null);
  const [isProcessing, setIsProcessing] = useState(false);

  // Close when clicking outside
  useClickOutside(popoverRef, onClose, isOpen);

  // Group items for display
  const groupedItems = useMemo(() => groupPendingItems(pendingItems), [pendingItems]);

  // Handle cancel/remove from queue
  const handleCancel = useCallback(async (item: GroupedQueueItem) => {
    if (isProcessing) return;
    setIsProcessing(true);
    
    try {
      if (item.group_id) {
        // Cancel entire shard group
        await cancelShardGroup(item.group_id);
      } else {
        // Remove single item
        await removeFromQueue(item.id);
      }
      onRefresh?.();;
    } catch (error) {
      console.error('Failed to remove from queue:', error);
    } finally {
      setIsProcessing(false);
    }
  }, [isProcessing, onRefresh]);

  // Move item up in queue (swap with previous item)
  const handleMoveUp = useCallback(async (index: number) => {
    if (isProcessing || index === 0) return;
    
    setIsProcessing(true);
    
    // Build new order by swapping current item with the one above
    const newOrder = groupedItems.map(item => item.id);
    [newOrder[index - 1], newOrder[index]] = [newOrder[index], newOrder[index - 1]];
    
    try {
      await reorderQueue(newOrder);
      onRefresh?.();
    } catch (error) {
      console.error('Failed to reorder queue:', error);
    } finally {
      setIsProcessing(false);
    }
  }, [groupedItems, isProcessing, onRefresh]);

  // Move item down in queue (swap with next item)
  const handleMoveDown = useCallback(async (index: number) => {
    if (isProcessing || index >= groupedItems.length - 1) return;
    
    setIsProcessing(true);
    
    // Build new order by swapping current item with the one below
    const newOrder = groupedItems.map(item => item.id);
    [newOrder[index], newOrder[index + 1]] = [newOrder[index + 1], newOrder[index]];
    
    try {
      await reorderQueue(newOrder);
      onRefresh?.();
    } catch (error) {
      console.error('Failed to reorder queue:', error);
    } finally {
      setIsProcessing(false);
    }
  }, [groupedItems, isProcessing, onRefresh]);

  if (!isOpen || groupedItems.length === 0) {
    return null;
  }

  return (
    <div className={styles.popover} ref={popoverRef}>
      <div className={styles.header}>
        <span className={styles.title}>Download Queue</span>
        <span className={styles.count}>{groupedItems.length} {groupedItems.length === 1 ? 'item' : 'items'}</span>
      </div>
      <div className={styles.content}>
        {groupedItems.map((item, index) => (
          <div
            key={item.group_id || item.id}
            className={styles.queueItem}
          >
            {/* Reorder buttons */}
            <div className={styles.reorderButtons}>
              <button
                className={styles.reorderBtn}
                onClick={() => handleMoveUp(index)}
                disabled={isProcessing || index === 0}
                title="Move up"
                aria-label="Move up in queue"
              >
                <Icon icon={ChevronUp} size={16} />
              </button>
              <button
                className={styles.reorderBtn}
                onClick={() => handleMoveDown(index)}
                disabled={isProcessing || index === groupedItems.length - 1}
                title="Move down"
                aria-label="Move down in queue"
              >
                <Icon icon={ChevronDown} size={16} />
              </button>
            </div>
            
            {/* Item info */}
            <div className={styles.itemInfo}>
              <div className={styles.modelName} title={item.id}>
                {item.display_name}
              </div>
              <div className={styles.itemMeta}>
                {item.shard_count > 1 && (
                  <span className={styles.shardBadge}>
                    {item.shard_count} parts
                  </span>
                )}
              </div>
            </div>
            
            {/* Cancel button */}
            <button
              className={styles.cancelBtn}
              onClick={() => handleCancel(item)}
              disabled={isProcessing}
              title="Remove from queue"
            >
              <Icon icon={X} size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
};

export default DownloadQueuePopover;
