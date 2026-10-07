import { FC, useState } from 'react';
import { Box, CheckCircle2, Download, RotateCcw } from 'lucide-react';
import type { QueueSnapshot } from '../../services/transport/types/downloads';
import type { QueueRunSummary } from '../../services/transport/types/events';
import DownloadQueuePopover from './DownloadQueuePopover';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Stack } from '../primitives';
import { cn } from '../../utils/cn';
import { Chip } from '../ui/Chip';

interface GlobalDownloadStatusProps {
  /** The download queue from useDownloadManager, or null before it is known */
  snapshot: QueueSnapshot | null;
  /** The download a cancel is out for, if any */
  cancellingId: string | null;
  /** Summary of last completed queue run (null if none or dismissed) */
  lastQueueSummary: QueueRunSummary | null;
  /** Callback to cancel the running download, by its id */
  onCancel: (id: string) => void;
  /** Callback when user dismisses completion summary */
  onDismissSummary: () => void;
  /** Callback to refresh queue status */
  onRefreshQueue?: () => void;
}

/**
 * Global download status component for page-level display.
 * Shows:
 * - The running download: one bar, and the row's own words around it
 * - How many downloads wait behind it, with a popover to manage them
 * - Completion summary with ALL downloaded models from queue run (dismissible)
 *
 * The card is the snapshot's `active` row and nothing else. Every string on
 * it about the download is the row's `text`, printed as it arrived, which is
 * what the CLI prints for the same download. The chip's count is the number
 * of waiting rows.
 */
const GlobalDownloadStatus: FC<GlobalDownloadStatusProps> = ({
  snapshot,
  cancellingId,
  lastQueueSummary,
  onCancel,
  onDismissSummary,
  onRefreshQueue,
}) => {
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  
  const row = snapshot?.active;
  const waiting = snapshot?.waiting ?? [];

  // The last run's summary shows once nothing is running
  if (lastQueueSummary && !row) {
    const downloaded = lastQueueSummary.items.filter(
      (item) => item.last_result === 'downloaded'
    );
    const totalAttempts =
      lastQueueSummary.total_attempts_downloaded +
      lastQueueSummary.total_attempts_failed +
      lastQueueSummary.total_attempts_cancelled;
    const uniqueTotal = lastQueueSummary.unique_models_downloaded;
    const hasRetries = totalAttempts > uniqueTotal;

    // Only show banner if at least one model was downloaded
    if (uniqueTotal === 0) {
      return null;
    }

    // Show first 3 items from the downloaded list
    const displayItems = downloaded.slice(0, 3);
    const shownCount = displayItems.length;
    // Remaining = unique total minus what we're showing
    const remaining = Math.max(0, uniqueTotal - shownCount);

    return (
      <div className="bg-background border-b border-border-light rounded-none p-base mb-0">
        <div className="flex flex-col gap-sm">
          <div className="flex items-center gap-sm">
            <span className="text-success" aria-hidden>
              <Icon icon={CheckCircle2} size={16} />
            </span>
            <span className="text-sm font-semibold text-text">
              {uniqueTotal === 1 ? 'Download Complete' : `${uniqueTotal} Downloads Complete`}
            </span>
          </div>
          <Stack gap="xs" className="p-sm bg-surface-elevated rounded-base max-h-[120px] overflow-y-auto">
            {displayItems.length > 0 ? (
              <>
                {displayItems.map((item, idx) => (
                  <div key={idx} className="text-sm text-text py-xs">
                    <span className="text-sm" aria-hidden>
                      <Icon icon={Box} size={14} />
                    </span>
                    {item.display_name}
                  </div>
                ))}
                {remaining > 0 && (
                  <div className="text-sm text-text py-xs">
                    …and {remaining} more
                  </div>
                )}
              </>
            ) : (
              <div className="text-sm text-text py-xs">
                <span className="text-sm" aria-hidden>
                  <Icon icon={Box} size={14} />
                </span>
                {uniqueTotal} {uniqueTotal === 1 ? 'model' : 'models'} downloaded
                {lastQueueSummary.truncated && ' (details truncated)'}
              </div>
            )}
          </Stack>
          {hasRetries && (
            <div className="text-sm text-text-secondary">
              <span className="text-sm" aria-hidden>
                <Icon icon={RotateCcw} size={14} />
              </span>
              {totalAttempts} total attempts
            </div>
          )}
          <Button variant="secondary" size="sm" className="self-end" onClick={onDismissSummary}>
            OK
          </Button>
        </div>
      </div>
    );
  }

  if (!row) return null;

  const { text } = row;
  const isCancelling = cancellingId === row.id;

  return (
    <div className="bg-background border-b border-border-light rounded-none p-base mb-0">
      <div className="flex flex-col gap-sm">
        <div className="flex items-center justify-between gap-md">
          <div className="flex items-center gap-sm">
            <span className="text-lg" aria-hidden>
              <Icon icon={Download} size={16} />
            </span>
            <span className="text-sm font-medium text-text">
              {text.status}
            </span>
            {text.file && (
              <span className="text-sm font-mono tabular-nums text-text-secondary">
                {text.file}
              </span>
            )}
            {waiting.length > 0 && (
              <div className="relative">
                <Chip
                  variant="primary"
                  size="sm"
                  onClick={() => setIsQueuePopoverOpen((prev) => !prev)}
                  title="Click to view and manage queue"
                >
                  +{waiting.length} queued
                </Chip>
                <DownloadQueuePopover
                  isOpen={isQueuePopoverOpen}
                  onClose={() => setIsQueuePopoverOpen(false)}
                  waiting={waiting}
                  onRefresh={onRefreshQueue}
                />
              </div>
            )}
          </div>
          <Button
            variant="dangerGhost"
            size="sm"
            onClick={() => onCancel(row.id)}
            disabled={isCancelling}
          >
            {isCancelling ? 'Cancelling...' : 'Cancel'}
          </Button>
        </div>

        <div className="text-sm text-text-secondary font-mono overflow-hidden text-ellipsis whitespace-nowrap" title={text.title}>
          {text.title}
        </div>

        <div className="flex items-center gap-sm">
          <div className="flex-1 h-2 bg-surface-elevated rounded-sm overflow-hidden">
            <div
              className={cn(
                'h-full bg-primary rounded-sm transition-[width] duration-200 ease-linear',
                row.percent === undefined && 'w-[30%] animate-indeterminate'
              )}
              style={row.percent !== undefined ? { width: `${row.percent}%` } : {}}
            />
          </div>
          <span className="text-sm font-mono font-medium tabular-nums text-text min-w-[48px] text-right">
            {text.percent}
          </span>
        </div>

        {/*
          Bytes, speed and time remaining, in the row's words. The speed and
          the time remaining are empty once the bytes are in, while the model
          is finalized and registered.
        */}
        <div className="flex gap-lg flex-wrap min-h-[1.25rem] text-sm font-mono tabular-nums text-text">
          <span>{text.bytes}</span>
          <span>{text.speed}</span>
          <span>{text.eta}</span>
        </div>
      </div>
    </div>
  );
};

export default GlobalDownloadStatus;
