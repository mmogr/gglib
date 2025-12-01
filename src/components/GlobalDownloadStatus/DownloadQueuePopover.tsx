import { FC, useRef, useState, useCallback, useMemo } from 'react';
import { DownloadQueueItem } from '../../types';
import { useClickOutside } from '../../hooks/useClickOutside';
import { TauriService } from '../../services/tauri';
import styles from './DownloadQueuePopover.module.css';

/**
 * Grouped queue item for display - sharded downloads are collapsed into one entry
 */
interface GroupedQueueItem {
  /** Primary model_id for the group (or single item) */
  model_id: string;
  /** Quantization if available */
  quantization?: string | null;
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
  onRefresh: () => void;
}

/**
 * Groups pending queue items by group_id (for sharded models) or model_id.
 * Sharded downloads appear as a single entry with shard count indicator.
 */
function groupPendingItems(items: DownloadQueueItem[]): GroupedQueueItem[] {
  const groups = new Map<string, GroupedQueueItem>();
  
  for (const item of items) {
    // Use group_id for sharded, model_id for single items
    const key = item.group_id || item.model_id;
    
    if (!groups.has(key)) {
      groups.set(key, {
        model_id: item.model_id,
        quantization: item.quantization,
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
 * Format model display name from model_id
 * e.g., "unsloth/Qwen3-30B-A3B-GGUF" -> "Qwen3-30B-A3B-GGUF"
 */
function formatModelName(modelId: string): string {
  const parts = modelId.split('/');
  return parts.length > 1 ? parts[1] : modelId;
}

/**
 * Popover component showing queued downloads with drag-to-reorder and cancel functionality.
 * Sharded models are grouped and displayed as a single entry.
 */
const DownloadQueuePopover: FC<DownloadQueuePopoverProps> = ({
  isOpen,
  onClose,
  pendingItems,
  onRefresh,
}) => {
  const popoverRef = useRef<HTMLDivElement>(null);
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null);
  const [dropTargetIndex, setDropTargetIndex] = useState<number | null>(null);
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
        await TauriService.cancelShardGroup(item.group_id);
      } else {
        // Remove single item
        await TauriService.removeFromDownloadQueue(item.model_id);
      }
      onRefresh();
    } catch (error) {
      console.error('Failed to remove from queue:', error);
    } finally {
      setIsProcessing(false);
    }
  }, [isProcessing, onRefresh]);

  // Drag and drop handlers
  const handleDragStart = useCallback((e: React.DragEvent, index: number) => {
    setDraggedIndex(index);
    e.dataTransfer.effectAllowed = 'move';
    // Use a transparent image as drag preview (we'll show our own indicator)
    const img = new Image();
    img.src = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
    e.dataTransfer.setDragImage(img, 0, 0);
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent, index: number) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    if (draggedIndex !== null && draggedIndex !== index) {
      setDropTargetIndex(index);
    }
  }, [draggedIndex]);

  const handleDragLeave = useCallback(() => {
    setDropTargetIndex(null);
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent, targetIndex: number) => {
    e.preventDefault();
    setDropTargetIndex(null);
    
    if (draggedIndex === null || draggedIndex === targetIndex || isProcessing) {
      setDraggedIndex(null);
      return;
    }
    
    setIsProcessing(true);
    const draggedItem = groupedItems[draggedIndex];
    
    try {
      // Calculate the new position based on target index
      // The backend expects a 0-based position in the queue
      // Simply use the target index directly since our grouped items are already sorted by position
      const newPosition = targetIndex;
      
      await TauriService.reorderDownloadQueue(draggedItem.model_id, newPosition);
      onRefresh();
    } catch (error) {
      console.error('Failed to reorder queue:', error);
    } finally {
      setDraggedIndex(null);
      setIsProcessing(false);
    }
  }, [draggedIndex, groupedItems, isProcessing, onRefresh]);

  const handleDragEnd = useCallback(() => {
    setDraggedIndex(null);
    setDropTargetIndex(null);
  }, []);

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
            key={item.group_id || item.model_id}
            className={`${styles.queueItem} ${draggedIndex === index ? styles.dragging : ''} ${dropTargetIndex === index ? styles.dropTarget : ''}`}
            draggable={!isProcessing}
            onDragStart={(e) => handleDragStart(e, index)}
            onDragOver={(e) => handleDragOver(e, index)}
            onDragLeave={handleDragLeave}
            onDrop={(e) => handleDrop(e, index)}
            onDragEnd={handleDragEnd}
          >
            {/* Drag handle */}
            <div className={styles.dragHandle} title="Drag to reorder">
              <span className={styles.dragIcon}>☰</span>
            </div>
            
            {/* Item info */}
            <div className={styles.itemInfo}>
              <div className={styles.modelName} title={item.model_id}>
                {formatModelName(item.model_id)}
              </div>
              <div className={styles.itemMeta}>
                {item.quantization && (
                  <span className={styles.quantization}>{item.quantization}</span>
                )}
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
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
};

export default DownloadQueuePopover;
