import type { FC } from 'react';
import { Readout, Sparkline } from '../primitives';
import { useMetricHistory } from '../../hooks/useMetricHistory';
import { formatPerSecond } from '../../utils/formatPerSecond';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';

interface RequestThroughputProps {
  snapshot: DashboardSnapshot | null;
}

/**
 * Requests-per-second derived from the snapshot's cumulative `total_requests`
 * counter. The snapshot object itself is the tick, so a quiet proxy registers
 * as a zero rate rather than a frozen chart.
 *
 * Rendered as a full row (readout left, sparkline right) under the section
 * heading rather than beside it — the heading and a readout label are both
 * small type, and stacking keeps the hierarchy legible at the tray's 360px.
 */
export const RequestThroughput: FC<RequestThroughputProps> = ({ snapshot }) => {
  const rate = useMetricHistory(snapshot?.total_requests, { mode: 'rate', tick: snapshot });

  if (!snapshot) return null;

  return (
    <div className="flex items-center justify-between gap-md mb-sm">
      <Readout
        size="sm"
        label="Throughput"
        value={rate.latest != null ? formatPerSecond(rate.latest) : '—'}
        unit="req/s"
      />
      <Sparkline
        values={rate.values}
        width={96}
        ariaLabel="Request throughput, recent history"
        className="text-primary"
      />
    </div>
  );
};
