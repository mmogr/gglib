/**
 * One download, as a library row: its name at the left, and at the right
 * either a bar with how far it has got and how long is left, or the word for
 * what it is doing. Every word is the row's own `text`, made once in
 * `gglib_core::download::row` and printed by the CLI too (TX-13): the
 * percentage, the time left, "Queued", "Finalizing…". The bar's width is
 * the row's `percent`; with no total known there is no bar to fill.
 *
 * Hovering shows ×: on the running download it cancels, on a waiting one it
 * takes it off the queue. `Downloads` is the group of them, the running one
 * first, drawn over the rows.
 *
 * @module places/library/DownloadRow
 */

import type { FC } from 'react';
import { X } from 'lucide-react';
import { IconButton } from '../../components/ui/IconButton';
import { Icon } from '../../components/ui/Icon';
import type { DownloadRow as Row } from '../../services/transport/types/downloads';
import { useDownloads } from '../../services/downloadsRegistry';
import { cancelDownload, removeFromQueue } from '../../services/downloadsEvents';
import { Group } from './Group';
import { matchesDownload } from './groups';

interface DownloadRowProps {
  row: Row;
  /** Whether a cancel of this download is out. */
  cancelling: boolean;
  onCancel: (id: string) => void;
  onRemove: (id: string) => void;
}

export const DownloadRow: FC<DownloadRowProps> = ({ row, cancelling, onCancel, onRemove }) => {
  const queued = row.phase === 'queued';
  const transferring = row.phase === 'downloading' && row.percent !== undefined;
  const { text } = row;
  return (
    <div
      role="group"
      aria-label={text.title}
      className="group flex min-h-[36px] items-center gap-md rounded-base px-md py-sm hover:bg-background-hover focus-within:bg-background-hover"
    >
      <span className="min-w-0 flex-1 truncate text-sm text-text">
        {text.title}
        {text.file && <span className="ml-sm text-xs text-text-secondary">{text.file}</span>}
      </span>
      {transferring ? (
        <span className="flex shrink-0 items-center gap-sm text-xs text-text-secondary tabular-nums">
          <span
            role="progressbar"
            aria-label={`${text.title} downloaded`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={row.percent}
            className="h-[2px] w-[96px] overflow-hidden rounded-full bg-surface-elevated"
          >
            {/* Dynamic: the bar's fill is the row's percentage. */}
            <span className="block h-full bg-text-secondary" style={{ width: `${row.percent}%` }} />
          </span>
          <span>{[text.percent, row.eta_seconds !== undefined ? text.eta : ''].filter(Boolean).join(' · ')}</span>
        </span>
      ) : (
        <span className="shrink-0 text-xs text-text-secondary">{text.status}</span>
      )}
      <IconButton
        label={queued ? `Remove ${text.title} from the queue` : `Cancel ${text.title}`}
        size="sm"
        variant="ghost"
        className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        disabled={cancelling}
        onClick={() => (queued ? onRemove(row.id) : onCancel(row.id))}
      >
        <Icon icon={X} size={14} />
      </IconButton>
    </div>
  );
};

/** The Downloading group: the queue's rows that match `query`, or nothing. */
export const Downloads: FC<{ query: string }> = ({ query }) => {
  const { snapshot, cancellingId } = useDownloads();
  const rows = snapshot ? [...(snapshot.active ? [snapshot.active] : []), ...snapshot.waiting] : [];
  const shown = rows.filter((row) => matchesDownload(row, query));
  if (shown.length === 0) return null;
  return (
    <Group label="Downloading">
      {shown.map((row) => (
        <DownloadRow
          key={row.id}
          row={row}
          cancelling={cancellingId === row.id}
          onCancel={(id) => void cancelDownload(id)}
          onRemove={(id) => void removeFromQueue(id)}
        />
      ))}
    </Group>
  );
};
