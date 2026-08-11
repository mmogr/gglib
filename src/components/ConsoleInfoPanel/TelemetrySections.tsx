import type { FC } from 'react';
import { Readout, Sparkline, Stack } from '../primitives';
import { useMetricHistory } from '../../hooks/useMetricHistory';
import { formatRate } from '../../utils/formatRate';
import type { ServerMetrics } from './useServerMetrics';

interface TelemetryProps {
  /** Latest poll result; a fresh object per poll, used directly as the history tick. */
  metrics: ServerMetrics | null;
  /** Identity of the server run — clears histories across a stop/start cycle. */
  runKey: unknown;
  contextLength?: number;
}

/** One label/value stat row, matching the panel's existing type scale. */
const Row: FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="flex justify-between items-center gap-sm py-xs">
    <span className="text-sm text-text-muted">{label}</span>
    <span className="text-sm text-text font-mono tabular-nums">{value}</span>
  </div>
);

/**
 * Context usage as a readout + sparkline (fixed 0–100 domain, so the line
 * shows real movement rather than autoscaled noise). Thresholds match the
 * donut: 70 warning, 90 danger.
 */
export const ContextUsageSection: FC<TelemetryProps> = ({ metrics, runKey, contextLength }) => {
  const pct =
    metrics?.kvCacheUsageRatio != null
      ? Math.round(metrics.kvCacheUsageRatio * 100)
      : metrics?.nTokensMax != null && contextLength
        ? Math.round((metrics.nTokensMax / contextLength) * 100)
        : null;
  const history = useMetricHistory(pct, { tick: metrics, resetKey: runKey });

  return (
    <section className="flex flex-col gap-sm">
      <h3 className="m-0 text-sm font-semibold text-text">Context Usage</h3>
      {pct !== null ? (
        <Stack gap="xs">
          <Readout
            label="KV cache"
            value={pct}
            unit="%"
            intent={pct >= 90 ? 'danger' : pct >= 70 ? 'warning' : 'neutral'}
            trend={
              <Sparkline
                values={history.values}
                min={0}
                max={100}
                width={120}
                height={24}
                ariaLabel="Context usage, recent history"
                className="text-primary"
              />
            }
          />
          {(metrics?.kvCacheTokens != null || metrics?.nTokensMax != null) && (
            <span className="text-xs text-text-muted font-mono tabular-nums">
              {(metrics.kvCacheTokens ?? metrics.nTokensMax ?? 0).toLocaleString()} tokens
            </span>
          )}
        </Stack>
      ) : (
        <p className="text-xs text-text-muted m-0">No usage yet</p>
      )}
    </section>
  );
};

/** Generation rate readout plus the cumulative counters it derives from. */
export const StatisticsSection: FC<Omit<TelemetryProps, 'contextLength'>> = ({
  metrics,
  runKey,
}) => {
  const genRate = useMetricHistory(metrics?.predictedTokensTotal, {
    mode: 'rate',
    tick: metrics,
    resetKey: runKey,
  });

  if (!metrics) return null;

  return (
    <section className="flex flex-col gap-sm">
      <h3 className="m-0 text-sm font-semibold text-text">Statistics</h3>
      <Readout
        label="Generation"
        value={genRate.latest != null ? formatRate(genRate.latest) : '—'}
        unit="tok/s"
        trend={
          <Sparkline
            values={genRate.values}
            width={120}
            height={24}
            ariaLabel="Generation rate, recent history"
            className="text-primary"
          />
        }
      />
      <Stack gap="xs">
        <Row label="Prompt Tokens" value={metrics.promptTokensTotal.toLocaleString()} />
        <Row label="Generated Tokens" value={metrics.predictedTokensTotal.toLocaleString()} />
        <Row label="Active Requests" value={String(metrics.requestsProcessing)} />
      </Stack>
    </section>
  );
};
