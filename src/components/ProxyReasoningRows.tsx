/**
 * ProxyReasoningRows.
 *
 * The two reasoning controls, and the one thing that makes them different from
 * everything else on the sampling panel: **nothing echoes them.**
 *
 * `reasoning_effort` becomes a chat-template kwarg consumed at render time, and
 * `reasoning_budget_tokens` is parsed into `params.sampling` and then serialised
 * by nothing — `task_params::to_json` emits no `reasoning_budget_*` field in
 * either branch, confirmed against 49 `/slots` captures taken mid-generation
 * (ADR 0007 finding 7a). So where the rest of this panel compares gglib's intent
 * against llama-server's readback, these rows show gglib's *record* of what it
 * resolved, and say so in the server's own words rather than a paraphrase.
 *
 * # The rules this file exists to obey
 *
 * 1. **Blind is not agreement.** The blindness note is a warning carrying its
 *    reason, never a tick — the panel's own rule, applied to a blindness that
 *    is permanent rather than a fault. It is rendered *beside a value* rather
 *    than always, because a warning that qualifies nothing is one people learn
 *    to skip.
 * 2. **`not yet observed` is never `not supported`.** They license opposite
 *    conclusions: the second says every effort setting on this model is inert,
 *    the first says nobody has managed to ask. So the unknown state is a
 *    warning with its cause, and the negative one is neutral information — not
 *    a failure, since suppressing a level a template ignores is the system
 *    working.
 * 3. **A suppressed level is shown, marked.** Hiding it would lose the only
 *    record there is; showing it unmarked would report a control that went
 *    nowhere as though it had worked.
 *
 * @module components/ProxyReasoningRows
 */

import type { FC } from 'react';
import { Banner } from './ui/Banner';
import type {
  SamplingClientFieldNames,
  SamplingEffortSupport,
  SamplingReasoningReadback,
} from '../services/transport/types/dashboard';

/** One label/value row, matching the sampling panel's type scale. */
const Row: FC<{ label: string; value: string }> = ({ label, value }) => (
  <div className="flex items-baseline justify-between gap-md">
    <span className="text-xs text-text-muted">{label}</span>
    <span className="text-sm text-text font-mono tabular-nums">{value}</span>
  </div>
);

/**
 * Why an unreadable state is unknown, since the server sent no reason with it.
 *
 * Phrased as the CLI mirror phrases the same case
 * (`render_reasoning.rs::effort_support_label`), so the two surfaces cannot
 * describe one wire value in two ways.
 */
const UNRECOGNISED_REASON =
  'This proxy reported a support state this build does not recognise.';

/** The unknown state, with whatever cause there is for it. */
const TemplateSupportUnknown: FC<{ reason: string }> = ({ reason }) => (
  <Banner variant="warning" title="Template support for reasoning_effort is unknown">
    {reason} Nothing here says the level is ignored — only that nobody has been
    able to ask.
  </Banner>
);

/**
 * What the running model's template says about `reasoning_effort`.
 *
 * Three states, kept three — and **only `supported` may render as `yes`**.
 * Everything else is unknown, including a state this build has never heard of:
 * this component is a real HTTP client of a contract served by a separately
 * deployed proxy (see the module docs on `types/dashboard.ts`), so a newer
 * proxy can name a state whose union arm does not exist here. Reading such a
 * value as the affirmative row would let version skew alone report health, and
 * reading it as `not_supported` would tell the operator every effort setting on
 * the model is inert — the collapse ADR 0007 decision 3 forbids and the reason
 * the backend sends a tagged union rather than a bool. The CLI mirror keeps the
 * same floor with `EffortSupport::Unrecognised` (`#[serde(other)]`).
 */
const TemplateSupport: FC<{ support: SamplingEffortSupport }> = ({ support }) => {
  switch (support.state) {
    case 'supported':
      return <Row label="Template reads reasoning_effort" value="yes" />;
    case 'not_supported':
      return (
        <Banner variant="info" title="This model's template never reads reasoning_effort">
          A resolved level is suppressed before the request is sent, so setting one
          for this model changes nothing.
        </Banner>
      );
    case 'not_yet_observed':
      return <TemplateSupportUnknown reason={support.reason} />;
    default:
      // Unreachable in the type, reachable on the wire. TypeScript's union is a
      // claim about the build that compiled it, not about the proxy answering.
      return <TemplateSupportUnknown reason={UNRECOGNISED_REASON} />;
  }
};

/** What the most recent request resolved, with the rung that supplied it. */
const LatestRows: FC<{ reasoning: SamplingReasoningReadback }> = ({ reasoning }) => {
  const latest = reasoning.latest;

  // An absent record and a record naming neither control are different facts:
  // the first is about traffic, the second about configuration.
  if (!latest) {
    return (
      <p className="text-sm text-text-muted">
        No request has resolved either reasoning control yet.
      </p>
    );
  }

  const { effort, budget } = latest;

  if (!effort && !budget) {
    return (
      <p className="text-sm text-text-muted">
        The last request named neither reasoning control, so llama-server's own
        defaults applied.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-xs">
      {effort && !effort.suppressed && (
        <Row label="Effort" value={`${effort.level} (${effort.source})`} />
      )}
      {effort && effort.suppressed && (
        <Banner variant="warning" title="A resolved effort level was never sent">
          {effort.level}, from {effort.source} — deleted before sending because
          the observed template never reads the variable.
        </Banner>
      )}
      {budget && (
        <Row
          label="Budget"
          value={`${budget.tokens.toLocaleString()} tokens (${budget.source})`}
        />
      )}
      {/*
        The blindness, beside the value it qualifies. Permanent rather than a
        fault, and still a warning: a reader who takes these two rows for a
        readback has been told something this system cannot know.
      */}
      <Banner variant="warning" title="Neither reasoning control can be read back">
        {reasoning.wire_blind_reason}
      </Banner>
    </div>
  );
};

/**
 * Which of the client's own sampling fields were dropped, by name.
 *
 * The count above answers "how many"; only the name answers *"is gglib ignoring
 * the `reasoning_effort` I sent?"*, which is the question this record exists
 * for. Not a fault: `trust_client_sampling` is off by default, so every
 * client-supplied sampler value is binned by design.
 */
export const DroppedClientFields: FC<{ names?: SamplingClientFieldNames | null }> = ({ names }) => {
  if (!names || names.fields.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-col gap-xs">
      {names.fields.map((tally) => (
        <div key={tally.field} className="flex items-baseline gap-md">
          <span className="text-xs text-text-muted w-32 shrink-0">{tally.field}</span>
          <span className="text-sm text-text">
            {tally.discarded > 0 && `${tally.discarded.toLocaleString()} dropped as untrusted`}
            {tally.discarded > 0 && tally.rejected > 0 && ' · '}
            {tally.rejected > 0 && `${tally.rejected.toLocaleString()} unreadable as sent`}
          </span>
        </div>
      ))}
      {/*
        The tally is bounded on the server. A bound nobody can see is
        indistinguishable from a bound nobody hit, which would make the list
        above a claim it cannot support.
      */}
      {names.untracked > 0 && (
        <p className="text-sm text-text-muted">
          {names.untracked.toLocaleString()} further drops were not tracked by
          name — the server's tally is bounded.
        </p>
      )}
    </div>
  );
};

export interface ProxyReasoningRowsProps {
  /** `null`/`undefined` on a proxy that predates this field. */
  reasoning?: SamplingReasoningReadback | null;
}

export const ProxyReasoningRows: FC<ProxyReasoningRowsProps> = ({ reasoning }) => {
  if (!reasoning) {
    return null;
  }

  return (
    <div className="flex flex-col gap-sm">
      <TemplateSupport support={reasoning.effort_support} />
      <LatestRows reasoning={reasoning} />
    </div>
  );
};

export default ProxyReasoningRows;
