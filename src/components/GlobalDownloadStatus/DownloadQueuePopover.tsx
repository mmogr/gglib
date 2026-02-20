import { FC, useRef, useState, useMemo } from 'react';
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { appLogger } from '../../services/platform';
import { useClickOutside } from '../../hooks/useClickOutside';
import {
  cancelShardGroup,
  removeFromQueue,
  reorderQueueItem,
} from '../../services/clients/downloads';
import type { DownloadQueueItem } from '../../services/transport/types/downloads';
import { Icon } from '../ui/Icon';

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
  onRefresh?: () => void | Promise<void>;
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
  const handleCancel = async (item: GroupedQueueItem) => {
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
      appLogger.error('component.download', 'Failed to remove from queue', { error });
    } finally {
      setIsProcessing(false);
    }
  };

  // Move item up in queue (swap with previous item)
  const handleMoveUp = async (index: number) => {
    if (isProcessing || index === 0) return; // Can't move first item up
    
    setIsProcessing(true);
    
    const item = groupedItems[index];
    const newPosition = item.position - 1; // Move to previous position
    
    try {
      await reorderQueueItem(item.id, newPosition);
      await onRefresh?.();
    } catch (error) {
      appLogger.error('component.download', 'Failed to reorder queue', { error });
    } finally {
      setIsProcessing(false);
    }
  };

  // Move item down in queue (swap with next item)
  const handleMoveDown = async (index: number) => {
    if (isProcessing || index >= groupedItems.length - 1) return; // Can't move last item down
    
    setIsProcessing(true);
    
    const item = groupedItems[index];
    const newPosition = item.position + 1; // Move to next position
    
    try {
      await reorderQueueItem(item.id, newPosition);
      await onRefresh?.();
    } catch (error) {
      appLogger.error('component.download', 'Failed to reorder queue', { error });
    } finally {
      setIsProcessing(false);
    }
  };

  if (!isOpen || groupedItems.length === 0) {
    return null;
  }

  return (
    <div
      className="absolute top-full left-0 mt-xs bg-surface border border-border rounded-md shadow-[0_4px_16px_rgba(0,0,0,0.3)] min-w-[280px] max-w-[360px] z-popover overflow-hidden"
      ref={popoverRef}
    >
      <div className="flex items-center justify-between px-md py-sm border-b border-border bg-surface-elevated">
        <span className="text-sm font-semibold text-text-primary">Download Queue</span>
        <span className="text-xs text-text-secondary bg-surface px-2 py-[2px] rounded-sm">{groupedItems.length} {groupedItems.length === 1 ? 'item' : 'items'}</span>
      </div>
      <div className="max-h-[300px] overflow-y-auto scrollbar-thin">
        {groupedItems.map((item, index) => (
          <div
            key={item.group_id || item.id}
            className="flex items-center gap-sm px-md py-sm border-b border-border last:border-b-0 hover:bg-surface-hover transition-colors duration-150"
          >
            {/* Reorder buttons */}
            <div className="flex flex-col gap-[2px] shrink-0">
              <button
                className="flex items-center justify-center w-[20px] h-[14px] bg-transparent border border-border rounded-[3px] text-text-secondary cursor-pointer text-[8px] leading-none p-0 transition-all duration-150 hover:not-disabled:bg-surface-hover hover:not-disabled:text-text-primary hover:not-disabled:border-border-hover active:not-disabled:bg-primary active:not-disabled:text-surface active:not-disabled:border-primary disabled:opacity-30 disabled:cursor-not-allowed"
                onClick={() => handleMoveUp(index)}
                disabled={isProcessing || index === 0}
                title="Move up"
                aria-label="Move up in queue"
              >
                <Icon icon={ChevronUp} size={16} />
              </button>
              <button
                className="flex items-center justify-center w-[20px] h-[14px] bg-transparent border border-border rounded-[3px] text-text-secondary cursor-pointer text-[8px] leading-none p-0 transition-all duration-150 hover:not-disabled:bg-surface-hover hover:not-disabled:text-text-primary hover:not-disabled:border-border-hover active:not-disabled:bg-primary active:not-disabled:text-surface active:not-disabled:border-primary disabled:opacity-30 disabled:cursor-not-allowed"
                onClick={() => handleMoveDown(index)}
                disabled={isProcessing || index === groupedItems.length - 1}
                title="Move down"
                aria-label="Move down in queue"
              >
                <Icon icon={ChevronDown} size={16} />
              </button>
            </div>
            
            {/* Item info */}
            <div className="flex-1 min-w-0 flex flex-col gap-[2px]">
              <div className="text-sm font-medium text-text-primary overflow-hidden text-ellipsis whitespace-nowrap" title={item.id}>
                {item.display_name}
              </div>
              <div className="flex items-center gap-xs flex-wrap">
                {item.shard_count > 1 && (
                  <span className="text-xs bg-[rgba(139,92,246,0.15)] text-[#a78bfa] px-[6px] py-[1px] rounded-sm font-medium">
                    {item.shard_count} parts
                  </span>
                )}
              </div>
            </div>
            
            {/* Cancel button */}
            <button
              className="flex items-center justify-center w-6 h-6 bg-transparent border-none rounded-sm text-text-secondary cursor-pointer opacity-60 shrink-0 text-[12px] transition-all duration-150 hover:not-disabled:bg-[rgba(248,113,113,0.15)] hover:not-disabled:text-[#f87171] hover:not-disabled:opacity-100 disabled:cursor-not-allowed disabled:opacity-30"
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
