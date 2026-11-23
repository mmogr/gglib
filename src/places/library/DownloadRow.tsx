/**
 * One download, as a library row: its name at the left, and at the right
 * either a bar with how far it has got and how long is left, or the word for
 * what it is doing. Every word is the row's own `text`, made once in
 * `gglib_core::download::row` and printed by the CLI too (TX-13): the
 * percentage, the time left, "Queued", "Finalizing…". The bar's width is
 * the row's `percent`; with no total known there is no bar to fill.
 *
 * While a note stands in for progress (a reconnect, a part being checked),
 * the note takes the place of the time remaining.
 *
 * Hovering shows ×: on the running download it cancels, on a waiting one it
 * takes it off the queue. `Downloads` is the group of them, the running one
 * first, drawn over the rows, then each download that failed and is not
 * queued again: why, in the daemon's words, with Retry and ×. Retry runs
 * the same job again: an upgrade upgrades its model again, anything else
 * is queued again by its repository and quantization, as the ending names
 * them. One that gave up on a quiet connection or was cut off carries on
 * from its part on disk (an upgrade from its commit's own part); one whose
 * bytes failed their check starts over. × takes the ending off the queue,
 * so it does not come back. A download held until Hugging Face's rate
 * limit lifts is a row of its own (`HeldRow`) after the queue's, and a
 * failed one whose Retry is held shows only that row.
 *
 * @module places/library/DownloadRow
 */

import type { FC } from 'react';
import { X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { IconButton } from '../../components/ui/IconButton';
import { Icon } from '../../components/ui/Icon';
import type { DownloadRow as Row, FinishedDownload } from '../../services/transport/types/downloads';
import { useDownloads } from '../../services/downloadsRegistry';
import { useHeldDownloads } from '../../services/heldDownloads';
import { cancelDownload, queueDownload, removeFromQueue, upgradeAgain } from '../../services/downloadsEvents';
import { appLogger } from '../../services/platform';
import { Group } from './Group';
import { matchesDownload } from './groups';
import { HeldRow } from './HeldRow';

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
          {/* A note (a reconnect, a part being checked) stands in for the time remaining; a long one is cut, and whole on hover. */}
          <span className="max-w-[260px] truncate" title={text.notice}>
            {[text.percent, text.notice ?? (row.eta_seconds !== undefined ? text.eta : '')].filter(Boolean).join(' · ')}
          </span>
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

/**
 * A download that failed, as a row: its name, why in the daemon's words,
 * Retry, which runs the same job again, and × to take it off the queue.
 */
const FailedRow: FC<{ ended: FinishedDownload }> = ({ ended }) => {
  const why = ended.outcome.kind === 'failed' ? ended.outcome.error : ended.text;
  const retry = () => {
    const again = ended.upgrade_of !== undefined ? upgradeAgain(ended, ended.upgrade_of) : queueDownload(ended.model_id, ended.quantization);
    again.catch((error: unknown) => appLogger.error('service.download', 'Retry failed', { error }));
  };
  return (
    <div
      role="group"
      aria-label={ended.title}
      className="group flex min-h-[36px] items-center gap-md rounded-base px-md py-sm hover:bg-background-hover focus-within:bg-background-hover"
    >
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm text-text">{ended.title}</span>
        <span role="alert" className="break-words text-xs text-text-secondary">
          {why}
        </span>
      </span>
      <Button variant="link" size="sm" className="shrink-0 text-xs font-normal" onClick={retry}>
        Retry
      </Button>
      <IconButton
        label={`Dismiss ${ended.title}`}
        size="sm"
        variant="ghost"
        className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        onClick={() => void removeFromQueue(ended.id)}
      >
        <Icon icon={X} size={14} />
      </IconButton>
    </div>
  );
};

/**
 * The Downloading group: the queue's rows that match `query`, the downloads
 * held until the Hub's limit lifts, then the downloads that failed and are
 * neither queued nor held again, until each is taken off the queue; or
 * nothing.
 */
export const Downloads: FC<{ query: string }> = ({ query }) => {
  const { snapshot, cancellingId } = useDownloads();
  const held = useHeldDownloads();
  const rows = snapshot ? [...(snapshot.active ? [snapshot.active] : []), ...snapshot.waiting] : [];
  const shown = rows.filter((row) => matchesDownload(row, query));
  const heldShown = held.filter((h) => matchesDownload(h, query));
  const again = new Set([...rows.map((row) => row.id), ...held.map((h) => h.id)]);
  const failed = (snapshot?.finished ?? []).filter(
    (ended) => ended.outcome.kind === 'failed' && !again.has(ended.id) && matchesDownload(ended, query),
  );
  if (shown.length === 0 && heldShown.length === 0 && failed.length === 0) return null;
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
      {heldShown.map((h) => (
        <HeldRow key={h.id} held={h} />
      ))}
      {failed.map((ended) => (
        <FailedRow key={ended.id} ended={ended} />
      ))}
    </Group>
  );
};
