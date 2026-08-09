/**
 * ProxySamplingPanel.
 *
 * Did the sampling gglib resolved reach llama-server intact — and, first, is
 * anything in a position to know?
 *
 * The GUI face of `gglib_proxy::sampling_audit`, the Tier C organ ADR 0001
 * says makes the other tiers honest and which the sampling subsystem did not
 * have. It compares what the request pipeline resolved against what a slot
 * reports mid-generation, and reads the build's own defaults from `/props`.
 *
 * # The one rule this panel exists to obey
 *
 * **Blind is not agreement.** An instrument that cannot see reports zero
 * divergences, and so does one that sees everything and finds nothing wrong.
 * The backend keeps those apart as distinct states rather than a shared count,
 * and the whole point is lost if the UI collapses them back into a green tick.
 * So `blind` renders as a warning carrying its reason, never as health.
 *
 * The same discipline applies to the baseline check below it: an
 * `indeterminate` verdict is rendered as unknown, not as a match. Today every
 * field is indeterminate, because gglib passes sampler values as llama-server
 * launch flags and those overwrite the very `/props` table the check reads —
 * so a "matches" there would be gglib agreeing with itself.
 *
 * @module components/ProxySamplingPanel
 */

import type { FC } from 'react';
import { Banner } from './ui/Banner';
import type {
  SamplingAuditSnapshot,
  SamplingBaselineReport,
  SamplingDivergence,
} from '../services/transport/types/dashboard';

export interface ProxySamplingPanelProps {
  /** `null`/`undefined` on a proxy that predates this field. */
  audit?: SamplingAuditSnapshot | null;
}

/** One label/value row, matching the other proxy panels' type scale. */
function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline justify-between gap-md">
      <span className="text-xs text-text-muted">{label}</span>
      <span className="text-sm text-text tabular-nums">{value}</span>
    </div>
  );
}

/**
 * Trim a float that made a round trip through `f32` and JSON.
 *
 * `0.05f32` widened to `f64` is `0.05000000074505806`; printing that verbatim
 * would make every value look like a defect.
 */
export function formatValue(value: number): string {
  return Number(value.toPrecision(6)).toString();
}

/**
 * The liveness line.
 *
 * Deliberately never renders a bare count for a state that has no counts:
 * `blind` gets a warning banner with its cause, `not_yet_observed` gets a
 * neutral note, and only `comparing` shows numbers.
 */
const AuditStateLine: FC<{ audit: SamplingAuditSnapshot }> = ({ audit }) => {
  const { state } = audit;

  if (state.state === 'blind') {
    return (
      <Banner variant="warning" title="Sampling readback is blind">
        {state.reason} Nothing is being compared, so zero divergences below
        would mean nothing.
      </Banner>
    );
  }

  if (state.state === 'not_yet_observed') {
    return (
      <p className="text-sm text-text-muted">
        No request caught mid-generation yet. The readback samples the running
        slot once a second, so short turns are often missed entirely.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-xs">
      <Row label="Requests observed" value={state.comparisons.toLocaleString()} />
      <Row label="Diverged" value={state.divergences.toLocaleString()} />
      {audit.skipped_ambiguous > 0 && (
        <Row
          label="Unattributable"
          value={audit.skipped_ambiguous.toLocaleString()}
        />
      )}
    </div>
  );
};

/** The `/props` baseline check: has this build's own default table moved? */
const BaselineRows: FC<{ baseline?: SamplingBaselineReport | null }> = ({ baseline }) => {
  if (!baseline) {
    return (
      <p className="text-sm text-text-muted">
        Build defaults not read yet.
      </p>
    );
  }

  const drifted = baseline.fields.filter((f) => f.verdict.verdict === 'differs');

  if (drifted.length > 0) {
    return (
      <Banner variant="danger" title="This build's sampler defaults have moved">
        <ul className="flex flex-col gap-xs">
          {drifted.map((f) => (
            <li key={f.field} className="text-sm">
              {f.field}:{' '}
              {f.verdict.verdict === 'differs' &&
                `expected ${formatValue(f.verdict.expected)}, this build reports ${formatValue(
                  f.verdict.observed,
                )}`}
            </li>
          ))}
        </ul>
      </Banner>
    );
  }

  // Inconclusive is its own answer, not a quiet pass. The reason comes from
  // the backend so the UI never has to guess which of the several causes
  // applies.
  if (!baseline.conclusive) {
    const reason = baseline.fields.find(
      (f) => f.verdict.verdict === 'indeterminate',
    )?.verdict;
    return (
      <Banner variant="info" title="Build defaults could not be checked">
        {reason?.verdict === 'indeterminate' ? reason.reason : 'No field could be concluded on.'}
      </Banner>
    );
  }

  return (
    <p className="text-sm text-text-muted">
      All {baseline.fields.length} sampler defaults match the values this build
      was measured at.
    </p>
  );
};

/** The recent-divergence list. Rare by design, so a short list is enough. */
const DivergenceRows: FC<{ divergences: SamplingDivergence[] }> = ({ divergences }) => {
  if (divergences.length === 0) {
    return null;
  }
  return (
    <div className="flex flex-col gap-xs p-md rounded-base border border-border bg-surface-elevated">
      {divergences.map((d, i) => (
        <div key={`${d.field}-${i}`} className="flex items-baseline gap-md">
          <span className="text-xs text-text-muted w-32 shrink-0">{d.field}</span>
          <span className="text-sm text-text">
            sent {formatValue(d.sent)} · server reports {formatValue(d.observed)}
          </span>
          <span className="text-xs text-text-muted ml-auto">({d.provenance})</span>
        </div>
      ))}
    </div>
  );
};

export const ProxySamplingPanel: FC<ProxySamplingPanelProps> = ({ audit }) => {
  if (!audit) {
    return (
      <p className="text-sm text-text-muted">
        This proxy does not report the sampling readback.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-sm">
      <AuditStateLine audit={audit} />
      <DivergenceRows divergences={audit.recent_divergences} />
      <BaselineRows baseline={audit.baseline} />

      {/*
        Discarded client fields are not a fault — `trust_client_sampling` is
        off by default, so gglib drops every client-supplied sampling value on
        purpose. Shown because "gglib is ignoring my temperature" should be
        answerable from here rather than from the source.
      */}
      {audit.client_fields_discarded > 0 && (
        <Row
          label="Client values discarded"
          value={`${audit.client_fields_discarded.toLocaleString()} (trust_client_sampling is off)`}
        />
      )}
      {audit.client_fields_rejected > 0 && (
        <Row
          label="Client values unreadable"
          value={audit.client_fields_rejected.toLocaleString()}
        />
      )}
    </div>
  );
};

export default ProxySamplingPanel;
