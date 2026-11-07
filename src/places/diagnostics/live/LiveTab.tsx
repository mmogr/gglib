/**
 * Diagnostics › Live: the endpoint's live numbers, each section drawn only
 * when it has something, in this order: Requests, Loaded and queue, Launch
 * decisions, Prompt cache (with Clear), Agent cache, Sampling readback,
 * Per-model signals, Tune activity, Slots, Loop-guard trips. The names of
 * the sections with nothing fold into one 12 px line under them.
 *
 * The frame is the one stream `dashboardRegistry` keeps for the endpoint
 * (`useEndpointDashboard`, the same target the sidebar and the endpoint
 * place read), so opening Live opens no second stream. Tune activity and
 * the loop guard's log are the daemon's reads, made when the tab opens.
 * While the endpoint is off, one sentence says the frame sections wait for
 * it; before its first frame, another says it is being read.
 *
 * @module places/diagnostics/live/LiveTab
 */

import type { ReactNode } from 'react';
import { Button } from '../../../components/ui/Button';
import { formatCount } from '../../../utils/format';
import { useClearCache } from '../../endpoint/useClearCache';
import { Group, Note } from '../../library/Group';
import { useEndpointDashboard } from '../../library/useResidency';
import type { DashboardSnapshot } from '../../../services/transport/types/dashboard';
import { LoopGuardTripsPanel } from './LoopGuardTripsPanel';
import { ProxyAdmissionPanel } from './ProxyAdmissionPanel';
import { CacheUsageRows, ProxyCachePanel } from './ProxyCachePanel';
import { ProxyLaunchPanel } from './ProxyLaunchPanel';
import { ModelSignalsSection, RequestsSection, SlotsSection } from './ProxyMetricsGrid';
import { ProxySamplingPanel } from './ProxySamplingPanel';
import {
  hasAgentCache,
  hasLaunch,
  hasLoaded,
  hasPromptCache,
  hasRequests,
  hasSampling,
  hasSignals,
  hasSlots,
} from './sections';
import { TuneActivityCard, useTuneActivity } from './TuneActivityCard';
import { useLoopGuardTrips } from './useLoopGuardTrips';

/** A section read from the frame, drawn only once there is one. */
interface FrameSection {
  name: string;
  frame: true;
  has: (s: DashboardSnapshot) => boolean;
  aside?: (s: DashboardSnapshot) => ReactNode;
  body: (s: DashboardSnapshot) => ReactNode;
}

/** A section from one of the daemon's reads, drawn whatever the endpoint does. */
interface ReadSection {
  name: string;
  frame: false;
  has: boolean;
  body: () => ReactNode;
}

export function LiveTab() {
  const dash = useEndpointDashboard();
  const tune = useTuneActivity();
  const trips = useLoopGuardTrips();
  const cache = useClearCache(dash);
  const s = dash.snapshot;

  const sections: (FrameSection | ReadSection)[] = [
    {
      name: 'Requests',
      frame: true,
      has: hasRequests,
      aside: (f) => <span className="font-normal tabular-nums">{`${formatCount(f.total_requests)} since the endpoint turned on`}</span>,
      body: (f) => <RequestsSection snapshot={f} />,
    },
    { name: 'Loaded and queue', frame: true, has: hasLoaded, body: (f) => <ProxyAdmissionPanel admission={f.admission} /> },
    { name: 'Launch decisions', frame: true, has: hasLaunch, body: (f) => <ProxyLaunchPanel launch={f.launch} /> },
    {
      name: 'Prompt cache',
      frame: true,
      has: hasPromptCache,
      aside: () => (
        <Button variant="link" size="sm" className="text-xs font-normal" disabled={!cache.canClear || cache.clearing} onClick={() => void cache.clear()}>
          Clear
        </Button>
      ),
      body: (f) => <ProxyCachePanel cache={f.cache} />,
    },
    // A separate population from the proxied figure above: GUI-chat runs
    // talk to llama-server directly, so their reuse is never averaged in.
    { name: 'Agent cache', frame: true, has: hasAgentCache, body: (f) => <CacheUsageRows usage={f.agent_usage} /> },
    { name: 'Sampling readback', frame: true, has: hasSampling, body: (f) => <ProxySamplingPanel audit={f.sampling_audit} /> },
    { name: 'Per-model signals', frame: true, has: hasSignals, body: (f) => <ModelSignalsSection snapshot={f} /> },
    { name: 'Tune activity', frame: false, has: tune !== null && tune.length > 0, body: () => <TuneActivityCard entries={tune ?? []} /> },
    { name: 'Slots', frame: true, has: hasSlots, body: (f) => <SlotsSection snapshot={f} /> },
    {
      name: 'Loop-guard trips',
      frame: false,
      // A failed read stays drawn, through its Retry's read, until one lands.
      has: (trips.days?.length ?? 0) > 0 || trips.error !== null,
      body: () => <LoopGuardTripsPanel state={trips} />,
    },
  ];
  // Without a frame the frame sections are not said to be empty: the line
  // above says why there is none.
  const empty = sections
    .filter((x) => (x.frame ? s !== null && !x.has(s) : !x.has))
    .map((x) => x.name);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="min-h-0 flex-1 overflow-y-auto py-md tabular-nums">
        {dash.port === null ? (
          <Note>The endpoint is off. Its live numbers show while it is on.</Note>
        ) : (
          s === null && <Note>Reading the endpoint&apos;s numbers…</Note>
        )}
        {sections.map((x) => {
          if (x.frame) {
            return s !== null && x.has(s) ? (
              <Group key={x.name} label={x.name} aside={x.aside?.(s)}>
                <div className="px-md">{x.body(s)}</div>
              </Group>
            ) : null;
          }
          return x.has ? (
            <Group key={x.name} label={x.name}>
              <div className="px-md">{x.body()}</div>
            </Group>
          ) : null;
        })}
        {empty.length > 0 && <Note>{`Nothing yet: ${empty.join(' · ')}.`}</Note>}
      </div>
    </div>
  );
}
