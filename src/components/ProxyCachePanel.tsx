/**
 * ProxyCachePanel.
 *
 * Prompt-cache section of the proxy dashboard: any configuration warnings the
 * backend raised, followed by measured reuse totals.
 *
 * Every figure shown is one the upstream actually reported. Nothing is
 * derived — in particular there is no "time saved", because reuse counts are
 * exact while what that reuse saved depends on a prefill that never ran. See
 * `gglib_proxy::cache_metrics` for the same reasoning on the backend.
 *
 * Extracted from `ProxyDashboardModal` to keep both files small and to make
 * the formatting logic testable without mounting the modal's `EventSource`
 * subscription.
 *
 * @module components/ProxyCachePanel
 */

import type { FC } from 'react';
import type { CacheStatus } from '../services/transport/types/dashboard';

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
      <span className="text-sm text-text tabular-nums">{value}</span>
    </div>
  );
}

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

  const { usage } = cache;
  const budget = ramBudgetLabel(cache);
  // Distinguishes "nothing measured yet" from "measured, and it was zero" —
  // the backend keeps those apart, so the UI must not merge them.
  const hasMeasurements = usage.reporting_requests > 0;

  return (
    <div className="flex flex-col gap-sm">
      {cache.warnings.length > 0 && (
        <div className="flex flex-col gap-xs p-md rounded-base border border-warning-border bg-warning-subtle">
          {cache.warnings.map((warning) => (
            <p key={warning} className="text-xs text-text-secondary">
              {warning}
            </p>
          ))}
        </div>
      )}

      <div className="flex flex-col gap-xs p-md rounded-base border border-border bg-surface-elevated">
        {hasMeasurements ? (
          <>
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
        {usage.unreported_requests > 0 && (
          <Row
            label="Requests without cache data"
            value={formatCount(usage.unreported_requests)}
          />
        )}
      </div>

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
