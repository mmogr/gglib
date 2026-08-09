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
 * The same discipline applies to the baseline check below it, twice over. An
 * `indeterminate` verdict renders as unknown, never as a match. And a read
 * that was attempted and failed renders as a warning carrying its cause, never
 * as the neutral "not read yet" — which is a statement about the poller, not
 * about the server.
 *
 * gglib no longer passes sampler values as launch flags (ADR 0003), which is
 * what un-blinded this check: a flag overwrites the very `/props` field it
 * names, so while they were passed a "matches" here was gglib agreeing with
 * itself and every field reported `indeterminate` by design.
 *
 * @module components/ProxySamplingPanel
 */

import type { FC } from 'react';
import { Banner } from './ui/Banner';
import type {
  SamplingAuditSnapshot,
  SamplingBaselineReport,
  SamplingBaselineState,
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
const BaselineRows: FC<{ baseline?: SamplingBaselineState | null }> = ({ baseline }) => {
  // A proxy that predates the field sends nothing; a current one always sends
  // a state, and `not_yet_read` is the honest answer only when no read has
  // been attempted.
  if (!baseline || baseline.state === 'not_yet_read') {
    return (
      <p className="text-sm text-text-muted">
        Build defaults not read yet.
      </p>
    );
  }

  // A read that was attempted and failed is not a read that has not happened.
  // Same rule as the liveness line above: the cause comes from the backend, so
  // the UI never has to guess which of the several failures applies.
  if (baseline.state === 'unreadable') {
    return (
      <Banner variant="warning" title="Build defaults could not be read">
        {baseline.reason} Until this succeeds, nothing below is checked against
        the table this build was measured at.
      </Banner>
    );
  }

  const { report } = baseline;
  const drifted = report.fields.filter((f) => f.verdict.verdict === 'differs');

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

  // Coverage second, and drift first above, so a partial reading can still
  // raise an alarm and a complete one is never silenced by the check below.
  const { coverage } = report;

  if (coverage.coverage === 'blind') {
    return (
      <Banner variant="warning" title="Build defaults could not be checked">
        <ReasonList report={report} />
      </Banner>
    );
  }

  if (coverage.coverage === 'partial') {
    return (
      <Banner
        variant="info"
        title={`Checked ${coverage.checked} of ${report.fields.length} sampler defaults`}
      >
        <ReasonList report={report} />
      </Banner>
    );
  }

  // `complete` still reports what the model supplied, when it supplied
  // nothing: the all-clear below is about the build's table, and a reader
  // should not have to infer that no field was model-supplied.

  // The only branch permitted to render an all-clear: every field compared.
  return (
    <p className="text-sm text-text-muted">
      All {report.fields.length} sampler defaults match the values this build
      was measured at.
    </p>
  );
};

/**
 * Why each unchecked field could not be concluded on.
 *
 * Groups by reason rather than showing the first one. There are several
 * distinct causes now, and `.find(...)` picked whichever happened to sort
 * first — which is not necessarily the interesting one.
 */
const ReasonList: FC<{ report: SamplingBaselineReport }> = ({ report }) => {
  const groups = new Map<string, string[]>();
  const supplied = report.fields.filter((f) => f.verdict.verdict === 'model_supplied');
  for (const f of report.fields) {
    if (f.verdict.verdict === 'indeterminate') {
      const fields = groups.get(f.verdict.reason) ?? [];
      fields.push(f.field);
      groups.set(f.verdict.reason, fields);
    }
  }
  if (groups.size === 0 && supplied.length === 0) {
    return <>No field could be concluded on.</>;
  }
  return (
    <ul className="flex flex-col gap-xs">
      {/*
        Model-supplied first: it is the benign explanation, and reading it
        after a list of failures makes an ordinary model look broken. This is
        llama.cpp applying the model author's own recommendation, which gglib
        defers to by design — it just means the build's own value for that
        field is not observable here.
      */}
      {supplied.map((f) => (
        <li key={f.field} className="text-sm">
          {f.field}:{' '}
          {f.verdict.verdict === 'model_supplied' &&
            `${formatValue(f.verdict.value)}, set by this model's own ${f.verdict.key}`}
        </li>
      ))}
      {[...groups].map(([reason, fields]) => (
        <li key={reason} className="text-sm">
          {fields.join(', ')}: {reason}
        </li>
      ))}
    </ul>
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
