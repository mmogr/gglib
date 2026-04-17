/**
 * Small badge indicating agent diff status after a refinement.
 *
 * Renders "NEW" or "MODIFIED" with an auto-fade animation.
 * Disappears after ~3 seconds unless the parent unmounts first.
 *
 * @module components/Council/Setup/AgentDiffBadge
 */

import { type FC, useState, useEffect } from 'react';
import { cn } from '../../../utils/cn';

export type DiffStatus = 'new' | 'modified' | 'unchanged';

interface AgentDiffBadgeProps {
  status: DiffStatus;
}

const labels: Record<DiffStatus, string | null> = {
  new: 'NEW',
  modified: 'MODIFIED',
  unchanged: null,
};

const colors: Record<DiffStatus, string> = {
  new: 'bg-success/20 text-success',
  modified: 'bg-warning/20 text-warning',
  unchanged: '',
};

export const AgentDiffBadge: FC<AgentDiffBadgeProps> = ({ status }) => {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    if (status === 'unchanged') return;
    const timer = setTimeout(() => setVisible(false), 3000);
    return () => clearTimeout(timer);
  }, [status]);

  const label = labels[status];
  if (!label || !visible) return null;

  return (
    <span
      className={cn(
        'text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded-full',
        'transition-opacity duration-500',
        colors[status],
        !visible && 'opacity-0',
      )}
    >
      {label}
    </span>
  );
};
