/**
 * A download held until Hugging Face's rate limit lifts, as a library row:
 * its repository and quantization, the daemon's words for a download that
 * waits (`waiting for Hugging Face's rate limit, 2:14`, counting down), and
 * × to drop it, which sends nothing. When the limit lifts its request is
 * sent again and the row says "queueing…", with no ×, until the queue's
 * own row takes its place.
 *
 * @module places/library/HeldRow
 */

import type { FC } from 'react';
import { X } from 'lucide-react';
import { waitingSentence } from '../../components/status/hubWait';
import { Icon } from '../../components/ui/Icon';
import { IconButton } from '../../components/ui/IconButton';
import { useSecondsLeft } from '../../hooks/useSecondsLeft';
import { dropHeld, type HeldDownload } from '../../services/heldDownloads';

export const HeldRow: FC<{ held: HeldDownload }> = ({ held }) => {
  const left = useSecondsLeft(held.liftsAt);
  return (
    <div
      role="group"
      aria-label={held.repo}
      className="group flex min-h-[36px] items-center gap-md rounded-base px-md py-sm hover:bg-background-hover focus-within:bg-background-hover"
    >
      <span className="min-w-0 flex-1 truncate text-sm text-text">
        {held.repo}
        {held.quantization && <span className="ml-sm text-xs text-text-secondary">{held.quantization}</span>}
      </span>
      <span role="status" className="shrink-0 text-xs text-text-secondary tabular-nums">
        {left === null ? 'queueing…' : waitingSentence(left)}
      </span>
      {left !== null && (
        <IconButton
          label={`Drop ${held.repo}`}
          size="sm"
          variant="ghost"
          className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
          onClick={() => dropHeld(held.id)}
        >
          <Icon icon={X} size={14} />
        </IconButton>
      )}
    </div>
  );
};
