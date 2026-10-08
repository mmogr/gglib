import { FC, useRef, useState } from 'react';
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { appLogger } from '../../services/platform';
import { useClickOutside } from '../../hooks/useClickOutside';
import type { DownloadRow } from '../../services/transport/types/downloads';
import { Icon } from '../ui/Icon';
import { Chip } from '../ui/Chip';
import { IconButton } from '../ui/IconButton';
import { getTransport } from '../../services/transport';

interface DownloadQueuePopoverProps {
  /** Whether the popover is open */
  isOpen: boolean;
  /** Called to close the popover */
  onClose: () => void;
  /** The snapshot's waiting rows, one per download, in the order they will run */
  waiting: DownloadRow[];
  /** Called after an item is removed/reordered to refresh queue */
  onRefresh?: () => void | Promise<void>;
}

/**
 * Popover component showing queued downloads with reorder and cancel functionality.
 * Uses up/down buttons for reordering (works in both Tauri WebKit and web browsers).
 * A row is a whole download, however many files it has; its name and its
 * "3 parts" are the row's own text.
 */
const DownloadQueuePopover: FC<DownloadQueuePopoverProps> = ({
  isOpen,
  onClose,
  waiting,
  onRefresh,
}) => {
  const popoverRef = useRef<HTMLDivElement>(null);
  const [isProcessing, setIsProcessing] = useState(false);

  // Close when clicking outside
  useClickOutside(popoverRef, onClose, isOpen);

  // Take a waiting download, every file of it, out of the queue
  const handleCancel = async (item: DownloadRow) => {
    if (isProcessing) return;
    setIsProcessing(true);
    
    try {
      await getTransport().removeFromQueue(item.id);
      onRefresh?.();
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
    
    const item = waiting[index];
    const newPosition = item.position - 1; // Move to previous position
    
    try {
      await getTransport().reorderQueueItem(item.id, newPosition);
      await onRefresh?.();
    } catch (error) {
      appLogger.error('component.download', 'Failed to reorder queue', { error });
    } finally {
      setIsProcessing(false);
    }
  };

  // Move item down in queue (swap with next item)
  const handleMoveDown = async (index: number) => {
    if (isProcessing || index >= waiting.length - 1) return; // Can't move last item down
    
    setIsProcessing(true);
    
    const item = waiting[index];
    const newPosition = item.position + 1; // Move to next position
    
    try {
      await getTransport().reorderQueueItem(item.id, newPosition);
      await onRefresh?.();
    } catch (error) {
      appLogger.error('component.download', 'Failed to reorder queue', { error });
    } finally {
      setIsProcessing(false);
    }
  };

  if (!isOpen || waiting.length === 0) {
    return null;
  }

  return (
    <div
      className="absolute top-full left-0 mt-xs bg-surface-elevated rounded-lg shadow-lg min-w-[280px] max-w-[360px] z-popover overflow-hidden"
      ref={popoverRef}
    >
      <div className="flex items-center justify-between px-md py-sm border-b border-border-light bg-surface-elevated">
        <span className="text-sm font-semibold text-text-primary">Download Queue</span>
        <span className="text-xs text-text-secondary bg-surface px-2 py-[2px] rounded-sm">{waiting.length} {waiting.length === 1 ? 'item' : 'items'}</span>
      </div>
      <div className="max-h-[300px] overflow-y-auto">
        {waiting.map((item, index) => (
          <div
            key={item.id}
            className="flex items-center gap-sm px-md py-sm hover:bg-surface-hover transition-colors duration-150"
          >
            {/* Reorder buttons */}
            <div className="flex flex-col gap-[2px] shrink-0">
              <IconButton
                label="Move up in queue"
                title="Move up"
                size="sm"
                className="h-5 w-5"
                onClick={() => handleMoveUp(index)}
                disabled={isProcessing || index === 0}
              >
                <Icon icon={ChevronUp} size={14} />
              </IconButton>
              <IconButton
                label="Move down in queue"
                title="Move down"
                size="sm"
                className="h-5 w-5"
                onClick={() => handleMoveDown(index)}
                disabled={isProcessing || index === waiting.length - 1}
              >
                <Icon icon={ChevronDown} size={14} />
              </IconButton>
            </div>
            
            {/* Item info */}
            <div className="flex-1 min-w-0 flex flex-col gap-[2px]">
              <div className="text-sm font-medium text-text-primary overflow-hidden text-ellipsis whitespace-nowrap" title={item.text.title}>
                {item.text.title}
              </div>
              <div className="flex items-center gap-xs flex-wrap">
                {item.text.file && (
                  <Chip variant="primary" size="sm" className="font-mono tabular-nums">
                    {item.text.file}
                  </Chip>
                )}
              </div>
            </div>
            
            {/* Cancel button */}
            <IconButton
              label="Remove from queue"
              size="sm"
              variant="dangerGhost"
              className="h-6 w-6 shrink-0"
              onClick={() => handleCancel(item)}
              disabled={isProcessing}
            >
              <Icon icon={X} size={14} />
            </IconButton>
          </div>
        ))}
      </div>
    </div>
  );
};

export default DownloadQueuePopover;
