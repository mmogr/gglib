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

import { useState, type ReactNode } from 'react';
import { Button } from '../../../components/ui/Button';
import { useToastContext } from '../../../contexts/ToastContext';
import { clearProxyCache } from '../../../services/clients/proxyDashboard';
import { formatCount } from '../../../utils/format';
import { formatError } from '../../../utils/errors';
import { Group, Note } from '../../library/Group';
import { useEndpointDashboard } from '../../library/useResidency';
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

interface Section {
  name: string;
  has: boolean;
  aside?: ReactNode;
  body: () => ReactNode;
}

export function LiveTab() {
  const dash = useEndpointDashboard();
  const { showToast } = useToastContext();
  const tune = useTuneActivity();
  const trips = useLoopGuardTrips();
  const [clearing, setClearing] = useState(false);
  const s = dash.snapshot;

  // Clear presents the proxy's key, so it waits for the reveal.
  const canClear = dash.port !== null && dash.apiKey !== undefined;
  const clear = async () => {
    if (dash.port === null || dash.apiKey === undefined) return;
    setClearing(true);
    try {
      const result = await clearProxyCache(dash.host, dash.port, dash.apiKey);
      showToast(result.message || 'Prompt cache cleared', 'success');
    } catch (err) {
      showToast(formatError(err), 'error');
    } finally {
      setClearing(false);
    }
  };

  const sections: Section[] = [
    {
      name: 'Requests',
      has: hasRequests(s),
      aside: s && <span className="font-normal tabular-nums">{`${formatCount(s.total_requests)} since the endpoint turned on`}</span>,
      body: () => <RequestsSection snapshot={s} />,
    },
    { name: 'Loaded and queue', has: hasLoaded(s), body: () => <ProxyAdmissionPanel admission={s?.admission} /> },
    { name: 'Launch decisions', has: hasLaunch(s), body: () => <ProxyLaunchPanel launch={s?.launch} /> },
    {
      name: 'Prompt cache',
      has: hasPromptCache(s),
      aside: (
        <Button variant="link" size="sm" className="text-xs font-normal" disabled={!canClear || clearing} onClick={() => void clear()}>
          Clear
        </Button>
      ),
      body: () => <ProxyCachePanel cache={s?.cache} />,
    },
    // A separate population from the proxied figure above: GUI-chat runs
    // talk to llama-server directly, so their reuse is never averaged in.
    { name: 'Agent cache', has: hasAgentCache(s), body: () => <CacheUsageRows usage={s?.agent_usage} /> },
    { name: 'Sampling readback', has: hasSampling(s), body: () => <ProxySamplingPanel audit={s?.sampling_audit} /> },
    { name: 'Per-model signals', has: hasSignals(s), body: () => <ModelSignalsSection snapshot={s} /> },
    { name: 'Tune activity', has: (tune?.length ?? 0) > 0, body: () => <TuneActivityCard entries={tune ?? []} /> },
    { name: 'Slots', has: hasSlots(s), body: () => <SlotsSection snapshot={s} /> },
    {
      name: 'Loop-guard trips',
      has: (trips.days?.length ?? 0) > 0 || trips.error !== null,
      body: () => <LoopGuardTripsPanel state={trips} />,
    },
  ];
  const empty = sections.filter((x) => !x.has).map((x) => x.name);

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="min-h-0 flex-1 overflow-y-auto py-md tabular-nums">
        {dash.port === null ? (
          <Note>The endpoint is off. Its live numbers show while it is on.</Note>
        ) : (
          s === null && <Note>Reading the endpoint&apos;s numbers…</Note>
        )}
        {sections
          .filter((x) => x.has)
          .map((x) => (
            <Group key={x.name} label={x.name} aside={x.aside}>
              <div className="px-md">{x.body()}</div>
            </Group>
          ))}
        {empty.length > 0 && <Note>{`Nothing yet: ${empty.join(' · ')}.`}</Note>}
      </div>
    </div>
  );
}
