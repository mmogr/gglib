/**
 * Renders the stance trajectory table after a round's judge evaluation.
 *
 * Shows each agent's stance (held / shifted / conceded) as a compact
 * row with a colour-coded badge. Designed to sit between the judge
 * assessment and the next round separator.
 *
 * @module components/Council/Messages/StanceTable
 */

import type { FC } from 'react';
import type { AgentStance, StanceTrajectory } from '../../../types/council';
import { cn } from '../../../utils/cn';

export interface StanceTableProps {
  stances: AgentStance[];
}

const trajectoryConfig: Record<StanceTrajectory, { label: string; className: string }> = {
  held:     { label: 'Held',     className: 'bg-sky-500/15 text-sky-600' },
  shifted:  { label: 'Shifted',  className: 'bg-amber-500/15 text-amber-600' },
  conceded: { label: 'Conceded', className: 'bg-rose-500/15 text-rose-600' },
};

export const StanceTable: FC<StanceTableProps> = ({ stances }) => {
  if (stances.length === 0) return null;

  return (
    <div className="flex flex-wrap items-center gap-sm px-md py-xs">
      <span className="text-xs font-medium text-text-muted">Stances:</span>
      {stances.map((s) => {
        const cfg = trajectoryConfig[s.trajectory];
        return (
          <span
            key={s.agent_name}
            className={cn(
              'inline-flex items-center gap-[4px] text-xs px-sm py-[1px] rounded-full font-medium',
              cfg.className,
            )}
          >
            {s.agent_name}
            <span className="opacity-70">·</span>
            {cfg.label}
          </span>
        );
      })}
    </div>
  );
};
