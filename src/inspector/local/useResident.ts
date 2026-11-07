/**
 * What a loaded model is doing, as its inspector's status line says it:
 * the residency word, its latest writing rate, and how long it has been
 * loaded ("● Working · 34 t/s · loaded 4m 12s").
 *
 * All three are the dashboard's (`useEndpointDashboard`): the resident
 * slot's state and `resident_for_secs`, and the newest recorded request on
 * the model (`readings/throughput`). Without the dashboard (the endpoint
 * off, its key not yet read) the word is the server list's launch state,
 * else Loaded, and the figures are left out.
 *
 * @module inspector/local/useResident
 */

import { RESIDENCY_WORDS } from '../../components/status/words';
import { useEndpointDashboard } from '../../places/library/useResidency';
import { latestRate, rateWords } from '../../readings/throughput';
import type { ResidencyState } from '../../types/generated/ResidencyState';
import { formatDuration } from '../../utils/format';

export interface Resident {
  state: ResidencyState;
  /** The status line: the word, then the figures the dashboard has. */
  line: string;
}

export function useResident(modelId: number, launchState: ResidencyState | undefined): Resident {
  const { snapshot } = useEndpointDashboard();
  const slot = snapshot?.admission.slots.find((s) => s.model_id === modelId);
  const state = slot?.launch?.state ?? launchState ?? 'loaded';
  const rate = slot ? latestRate(snapshot?.recent_requests ?? [], slot.model_name) : null;
  const line = [
    RESIDENCY_WORDS[state],
    rate !== null ? rateWords(rate) : null,
    slot ? `loaded ${formatDuration(slot.resident_for_secs)}` : null,
  ]
    .filter(Boolean)
    .join(' · ');
  return { state, line };
}
