/**
 * Lightweight horizontal divider with "Round N" label.
 *
 * Purely presentational — no state, no side-effects.
 *
 * @module components/Council/Messages/RoundSeparator
 */

import type { FC } from 'react';

export interface RoundSeparatorProps {
  /** Zero-based round index (displayed as round + 1). */
  round: number;
}

export const RoundSeparator: FC<RoundSeparatorProps> = ({ round }) => (
  <div className="flex items-center gap-md py-sm" role="separator">
    <div className="flex-1 h-px bg-border" />
    <span className="text-xs font-medium text-text-muted whitespace-nowrap">
      Round {round + 1}
    </span>
    <div className="flex-1 h-px bg-border" />
  </div>
);
