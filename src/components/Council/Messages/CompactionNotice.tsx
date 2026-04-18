/**
 * Renders a compaction notice in the council thread.
 *
 * Lightweight system-level notification indicating that a round's
 * full transcript was replaced by a summary to keep the context
 * window manageable.
 *
 * @module components/Council/Messages/CompactionNotice
 */

import type { FC } from 'react';
import { Minimize2 } from 'lucide-react';
import { Icon } from '../../ui/Icon';

export interface CompactionNoticeProps {
  round: number;
  summary: string;
}

export const CompactionNotice: FC<CompactionNoticeProps> = ({ round, summary }) => (
  <div className="flex items-start gap-sm px-md py-sm text-xs text-text-muted">
    <Icon icon={Minimize2} size={12} className="mt-[2px] shrink-0 opacity-60" />
    <div>
      <span className="font-medium">Round {round + 1} compacted</span>
      <span className="mx-[4px]">—</span>
      <span className="italic">{summary}</span>
    </div>
  </div>
);
