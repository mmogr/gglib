/**
 * ▸ Activity: the dashboard's other sections as short sentences (how much
 * of the prompts the cache served, tool calls repaired, loop-guard trips,
 * client sampling fields ignored), with Clear for the prompt cache and
 * Live numbers › for the whole dashboard.
 *
 * @module places/endpoint/Activity
 */

import type { FC } from 'react';
import { Button } from '../../components/ui/Button';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import { activitySentences } from './lines';

interface ActivityProps {
  snapshot: DashboardSnapshot;
  clearing: boolean;
  /** `false` until the key Clear presents has been revealed. */
  canClear: boolean;
  onClear: () => void;
  onLive: () => void;
}

export const Activity: FC<ActivityProps> = ({ snapshot, clearing, canClear, onClear, onLive }) => {
  const sentences = activitySentences(snapshot);
  return (
    <details className="px-md pb-md" aria-label="Activity">
      <summary className="cursor-pointer text-xs font-semibold text-text-secondary">Activity</summary>
      <div className="flex flex-wrap items-center gap-sm pt-sm text-xs text-text-secondary">
        <span>{sentences.length > 0 ? sentences.join(' · ') : 'Nothing to report yet.'}</span>
        <Button variant="link" size="sm" className="text-xs font-normal" onClick={onClear} disabled={!canClear || clearing}>
          {clearing ? 'Clearing…' : 'Clear prompt cache'}
        </Button>
        <Button variant="link" size="sm" className="text-xs font-normal" onClick={onLive}>
          Live numbers ›
        </Button>
      </div>
    </details>
  );
};
