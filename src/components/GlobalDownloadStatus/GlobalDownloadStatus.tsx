import { FC, useState } from 'react';
import { Box, CheckCircle2, Download, RotateCcw } from 'lucide-react';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import type { DownloadProgressView, DownloadUiState } from '../../hooks/useDownloadManager';
import type { QueueRunSummary } from '../../services/transport/types/events';
import { formatBytes, formatTime } from '../../utils/format';
import DownloadQueuePopover from './DownloadQueuePopover';
import { Icon } from '../ui/Icon';
import { Stack } from '../primitives';
import { cn } from '../../utils/cn';

interface GlobalDownloadStatusProps {
  /** Current download progress from useDownloadManager hook */
  progress: DownloadProgressView | null;
  /** Queue status from useDownloadManager hook */
  queueStatus: DownloadQueueStatus | null;
  /** Single source of truth for UI state (replaces derived currentId logic) */
  downloadUiState: DownloadUiState;
  /** Summary of last completed queue run (null if none or dismissed) */
  lastQueueSummary: QueueRunSummary | null;
  /** Callback to cancel the current download */
  onCancel: (modelId: string) => void;
  /** Callback when user dismisses completion summary */
  onDismissSummary: () => void;
  /** Callback to refresh queue status */
  onRefreshQueue?: () => void;
}

/**
 * Global download status component for page-level display.
 * Shows:
 * - Active download progress with shard support
 * - Queue status (X more queued)
 * - Completion summary with ALL downloaded models from queue run (dismissible)
 */
const GlobalDownloadStatus: FC<GlobalDownloadStatusProps> = ({
  progress,
  queueStatus,
  downloadUiState,
  lastQueueSummary,
  onCancel,
  onDismissSummary,
  onRefreshQueue,
}) => {
  const [isQueuePopoverOpen, setIsQueuePopoverOpen] = useState(false);
  
  // Single source of truth for what should be displayed
  const isActive = !!downloadUiState.activeId;
  const currentId = downloadUiState.activeId || '';
  const isCancelling = downloadUiState.phase === 'cancelling';
  const queueCount = queueStatus?.pending?.length || 0;

  // Show completion summary (priority over active progress)
  if (lastQueueSummary && !isActive) {
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
      <div className="bg-background border-b border-border rounded-none p-base mb-0">
        <div className="flex flex-col gap-sm">
          <div className="flex items-center gap-sm">
            <span className="text-[1.25rem]" aria-hidden>
              <Icon icon={CheckCircle2} size={16} />
            </span>
            <span className="text-base font-semibold text-success">
              {uniqueTotal === 1 ? 'Download Complete' : `${uniqueTotal} Downloads Complete`}
            </span>
          </div>
          <Stack gap="xs" className="p-sm bg-surface-raised rounded-base max-h-[120px] overflow-y-auto">
            {displayItems.length > 0 ? (
              <>
                {displayItems.map((item, idx) => (
                  <div key={idx} className="text-sm text-text py-xs border-b border-border last:border-b-0">
                    <span className="text-sm" aria-hidden>
                      <Icon icon={Box} size={14} />
                    </span>
                    {item.display_name}
                  </div>
                ))}
                {remaining > 0 && (
                  <div className="text-sm text-text py-xs border-b border-border last:border-b-0">
                    …and {remaining} more
                  </div>
                )}
              </>
            ) : (
              <div className="text-sm text-text py-xs border-b border-border last:border-b-0">
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
          <button className="self-end bg-[rgba(16,185,129,0.15)] text-success border border-[rgba(16,185,129,0.3)] rounded-base px-[1.25rem] py-[0.4rem] text-sm font-semibold cursor-pointer transition-all hover:bg-[rgba(74,222,128,0.25)] hover:border-[rgba(74,222,128,0.5)]" onClick={onDismissSummary}>
            OK
          </button>
        </div>
      </div>
    );
  }

  if (!isActive) return null;

  const percentage = progress?.percentage ?? undefined;
  const shard = progress?.shard;
  const isSharded = !!(shard && shard.total > 1);

  return (
    <div className="bg-background border-b border-border rounded-none p-base mb-0">
      <div className="flex flex-col gap-sm">
        <div className="flex items-center justify-between gap-md">
          <div className="flex items-center gap-sm">
            <span className="text-[1.1rem]" aria-hidden>
              <Icon icon={Download} size={16} />
            </span>
            <span className="text-sm font-medium text-text">
              {isSharded && shard ? `Downloading shard ${shard.index + 1}/${shard.total}` : 'Downloading'}
            </span>
            {queueCount > 0 && (
              <div className="relative">
                <button
                  className="bg-[rgba(34,211,238,0.15)] text-primary text-xs font-medium px-[0.5rem] py-[0.15rem] rounded-sm border-none cursor-pointer transition-all hover:bg-[rgba(34,211,238,0.25)]"
                  onClick={() => setIsQueuePopoverOpen((prev) => !prev)}
                  title="Click to view and manage queue"
                >
                  +{queueCount} queued
                </button>
                <DownloadQueuePopover
                  isOpen={isQueuePopoverOpen}
                  onClose={() => setIsQueuePopoverOpen(false)}
                  pendingItems={queueStatus?.pending || []}
                  onRefresh={onRefreshQueue}
                />
              </div>
            )}
          </div>
          {currentId && (
            <button 
              className="bg-[rgba(239,68,68,0.15)] text-danger border border-[rgba(239,68,68,0.3)] rounded-base px-[0.75rem] py-[0.3rem] text-sm font-medium cursor-pointer transition-all hover:bg-[rgba(239,68,68,0.25)] hover:border-[rgba(239,68,68,0.5)] disabled:opacity-50 disabled:cursor-not-allowed disabled:bg-[rgba(239,68,68,0.1)] disabled:border-[rgba(239,68,68,0.2)]"
              onClick={() => onCancel(currentId)}
              disabled={isCancelling}
            >
              {isCancelling ? 'Cancelling...' : 'Cancel'}
            </button>
          )}
        </div>

        <div className="text-sm text-text-secondary font-mono overflow-hidden text-ellipsis whitespace-nowrap" title={currentId}>
          {currentId.length > 50 ? `${currentId.substring(0, 47)}...` : currentId}
        </div>

        <div className="flex items-center gap-sm">
          <div className="flex-1 h-2 bg-surface-raised rounded-sm overflow-hidden">
            <div
              className={cn(
                'h-full bg-linear-to-r from-primary to-info rounded-sm transition-[width] duration-200 ease-linear',
                percentage === undefined && 'w-[30%] animate-indeterminate'
              )}
              style={percentage !== undefined ? { width: `${percentage}%` } : {}}
            />
          </div>
          <span className="text-sm font-semibold text-text min-w-[48px] text-right">
            {percentage !== undefined ? `${percentage.toFixed(1)}%` : '...'}
          </span>
        </div>

        <div className="flex gap-md flex-wrap">
          {progress?.downloaded !== undefined && progress?.total !== undefined && (
            <span className="text-xs text-text-secondary">
              {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
            </span>
          )}
          {progress?.speedBps !== undefined && (
            <span className="text-xs text-text-secondary">{formatBytes(progress.speedBps)}/s</span>
          )}
          {progress?.etaSeconds !== undefined && (
            <span className="text-xs text-text-secondary">ETA: {formatTime(progress.etaSeconds)}</span>
          )}
        </div>

        {isSharded && shard && (
          <div className="bg-surface-raised rounded-base p-sm mt-xs">
            <div className="flex items-center justify-between mb-xs">
              <span className="text-xs font-medium text-warning">
                Shard {shard.index + 1}/{shard.total}
              </span>
              {shard.filename && (
                <span className="text-xs text-text-secondary font-mono" title={shard.filename}>
                  {shard.filename.length > 25 ? `...${shard.filename.slice(-22)}` : shard.filename}
                </span>
              )}
            </div>
            <div className="h-1 bg-[rgba(255,255,255,0.1)] rounded-[2px] overflow-hidden">
              <div
                className="h-full bg-warning rounded-[2px] transition-[width] duration-200 ease-linear"
                style={{
                  width:
                    shard.totalBytes && shard.totalBytes > 0
                      ? `${((shard.downloaded || 0) / shard.totalBytes) * 100}%`
                      : '0%',
                }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default GlobalDownloadStatus;
