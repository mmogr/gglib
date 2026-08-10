/**
 * ProxyCachePanel.
 *
 * Prompt-cache section of the proxy dashboard: any configuration warnings the
 * backend raised, followed by measured reuse totals.
 *
 * Every figure shown is grounded in counts the upstream actually reported:
 * the hit-rate readouts are ratios of exact reported totals. Speculative
 * projections stay out — in particular there is no "time saved", because
 * reuse counts are exact while what that reuse saved depends on a prefill
 * that never ran. See `gglib_core::cache_metrics` for the same reasoning.
 *
 * Extracted from `ProxyDashboardModal` to keep both files small and to make
 * the formatting logic testable without mounting the modal's dashboard stream
 * subscription.
 *
 * @module components/ProxyCachePanel
 */

import type { FC } from 'react';
import { Banner } from './ui/Banner';
import { Readout } from './primitives';
import type { CacheStatus, CacheUsage } from '../services/transport/types/dashboard';

export interface ProxyCachePanelProps {
  /** `null`/`undefined` before the first request resolves a model. */
  cache?: CacheStatus | null;
}

/** Thousands-separated count, so six-figure token totals stay readable. */
function formatCount(value: number): string {
  return value.toLocaleString();
}

/** One label/value row, matching the modal's existing type scale. */
function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-md">
      <span className="text-xs text-text-muted">{label}</span>
      <span className="text-sm text-text font-mono tabular-nums">{value}</span>
    </div>
  );
}

/**
 * The measured-reuse rows for one cache population, shared by the proxied
 * figure and the agent-path figure.
 *
 * `hasMeasurements` distinguishes "nothing measured yet" from "measured, and it
 * was zero" — the backend keeps those apart, so the UI must not merge them.
 */
export const CacheUsageRows: FC<{ usage?: CacheUsage | null }> = ({ usage }) => {
  const hasMeasurements = (usage?.reporting_requests ?? 0) > 0;

  const hitRatePct =
    usage && hasMeasurements && usage.prompt_tokens > 0
      ? Math.round((usage.cached_tokens / usage.prompt_tokens) * 100)
      : null;
  const lastRatePct =
    usage && usage.last_cached_tokens != null && usage.last_prompt_tokens
      ? Math.round((usage.last_cached_tokens / usage.last_prompt_tokens) * 100)
      : null;

  return (
    <div className="flex flex-col gap-xs p-md rounded-base bg-surface-elevated">
      {usage && hasMeasurements ? (
        <>
          {(hitRatePct != null || lastRatePct != null) && (
            <div className="flex gap-xl pb-sm">
              {hitRatePct != null && <Readout label="Cache hit rate" value={hitRatePct} unit="%" />}
              {lastRatePct != null && (
                <Readout label="Last request reuse" value={lastRatePct} unit="%" />
              )}
            </div>
          )}
          <Row label="Used from cache" value={`${formatCount(usage.cached_tokens)} tokens`} />
          <Row label="Prompt tokens processed" value={`${formatCount(usage.prompt_tokens)} tokens`} />
          <Row label="Requests measured" value={formatCount(usage.reporting_requests)} />
          {usage.last_cached_tokens != null && usage.last_prompt_tokens != null && (
            <Row
              label="Last request"
              value={`${formatCount(usage.last_cached_tokens)} of ${formatCount(usage.last_prompt_tokens)} tokens from cache`}
            />
          )}
        </>
      ) : (
        <p className="text-sm text-text-muted">No cache activity recorded yet.</p>
      )}

      {/*
        Only shown when some requests went unmeasured, since on a current
        llama.cpp this is always zero and a permanent "0" row would be noise.
      */}
      {usage && usage.unreported_requests > 0 && (
        <Row label="Requests without cache data" value={formatCount(usage.unreported_requests)} />
      )}
    </div>
  );
};

/**
 * Human-readable summary of how the RAM budget resolved.
 *
 * Returns `null` for `llama_default`, where gglib emitted no flag and so has
 * no figure of its own to report.
 */
function ramBudgetLabel(cache: CacheStatus): string | null {
  switch (cache.ram_state) {
    case 'healthy':
    case 'low':
      return cache.ram_budget_mb != null ? `${formatCount(cache.ram_budget_mb)} MiB` : null;
    case 'disabled_by_user':
      return 'Disabled';
    case 'disabled_insufficient_ram':
      return 'Unavailable — not enough memory';
    case 'llama_default':
      return null;
  }
}

export const ProxyCachePanel: FC<ProxyCachePanelProps> = ({ cache }) => {
  if (!cache) {
    return <p className="text-sm text-text-muted">No model resolved yet.</p>;
  }

  const budget = ramBudgetLabel(cache);

  return (
    <div className="flex flex-col gap-sm">
      {cache.warnings.length > 0 && (
        <Banner variant="warning">
          <div className="flex flex-col gap-xs">
            {cache.warnings.map((warning) => (
              <p key={warning} className="m-0 text-xs">
                {warning}
              </p>
            ))}
          </div>
        </Banner>
      )}

      <CacheUsageRows usage={cache.usage} />

      <div className="flex flex-wrap gap-md text-xs text-text-muted">
        {budget && <span>RAM budget: {budget}</span>}
        <span>
          Disk cache:{' '}
          {!cache.disk_enabled ? 'off' : cache.disk_suppressed_for_model ? 'off for this model' : 'on'}
        </span>
      </div>
    </div>
  );
};

export default ProxyCachePanel;
