import type { FC } from 'react';
import { formatCount } from '../../utils/format';
import type { ModelDefectCounts } from '../../services/transport/types/dashboard';

/** `n` and `noun`, with the noun plural unless `n` is one. */
export const countOf = (n: number, noun: string): string =>
  `${formatCount(n)} ${noun}${n === 1 ? '' : 's'}`;

/**
 * Whether a model has anything to report: the CLI's `is_clean`, negated.
 *
 * `requests` and `agent_guard_scanned` are denominators, so a model with
 * nothing but those has nothing to say. The three repeat counters are not
 * faults and still count, because a model whose only signal is going in
 * circles is exactly the one to look at.
 */
export const hasSignal = (c: ModelDefectCounts): boolean =>
  [
    c.loop_guard_trips,
    c.agent_guard_trips,
    c.repairs_attempted,
    c.stream_errors,
    c.truncated_generations,
    c.empty_responses,
    c.dialect_residue,
    c.unvalidatable_schemas,
    c.normalization_errors,
    c.identical_result_repeats,
    c.repeats_not_evaluated,
    c.repeats_rescued,
  ].some((n) => n > 0);

/** One label/value row; `nested` indents it under the row it is part of. */
const Row: FC<{ label: string; value: string; nested?: boolean }> = ({ label, value, nested }) => (
  <div className={`flex items-baseline justify-between gap-md${nested ? ' pl-md' : ''}`}>
    <span className="text-xs text-text-muted">{label}</span>
    <span className="text-sm text-text font-mono tabular-nums">{value}</span>
  </div>
);

/** A row for each count above zero: a counter that never fired is not news. */
const firedRows = (counts: [string, number][], nested = false) =>
  counts
    .filter(([, n]) => n > 0)
    .map(([label, n]) => <Row key={label} label={label} value={formatCount(n)} nested={nested} />);

/**
 * One model's signals, under the rules `gglib proxy dashboard` prints them by
 * (`render_defects.rs`, whose tests are the specification).
 *
 * Only what fired. Repairs read as a ratio. Loop-guard trips show their sum,
 * then the detector that raised them. The agent path's trips sit in a row of
 * their own against the decisions they were taken over, since an agent turn
 * is not a proxy request; a model reached only that way says it had no proxy
 * requests rather than showing a zero. Reasoning-only turns are counted
 * inside the empty total, so they are shown as a share of it. The three
 * repeat counters are facts about the conversation rather than faults, so
 * they sit under an "Observed" heading of their own.
 */
export const ModelSignalsCard: FC<{ model: string; counts: ModelDefectCounts }> = ({
  model,
  counts: c,
}) => {
  const observed = firedRows(
    [
      ['Repeated, same result', c.identical_result_repeats],
      ['Repeated, not comparable', c.repeats_not_evaluated],
      ['Repeated, new result', c.repeats_rescued],
    ],
    true,
  );

  return (
    <div role="group" aria-label={model} className="flex flex-col gap-xs p-md rounded-base bg-surface-elevated">
      <div className="flex items-center justify-between gap-md mb-xs">
        <span className="text-sm font-semibold text-text truncate">{model}</span>
        <span className="text-xs text-text-muted">
          {c.requests === 0 && c.agent_guard_scanned > 0
            ? 'No proxy requests'
            : countOf(c.requests, 'request')}
        </span>
      </div>
      {c.repairs_attempted > 0 && (
        <Row
          label="Tool-call repairs"
          value={`${formatCount(c.repairs_succeeded)} of ${formatCount(c.repairs_attempted)} succeeded`}
        />
      )}
      {c.loop_guard_trips > 0 && (
        <>
          <Row label="Loop-guard trips" value={formatCount(c.loop_guard_trips)} />
          {firedRows(
            [
              ['Loop detector', c.loop_guard_loops],
              ['Stagnation detector', c.loop_guard_stagnations],
            ],
            true,
          )}
        </>
      )}
      {c.agent_guard_trips > 0 && (
        <>
          <Row
            label="Agent-path guard trips"
            value={`${formatCount(c.agent_guard_trips)} of ${countOf(c.agent_guard_scanned, 'decision')}`}
          />
          {firedRows(
            [
              ['Loop detector', c.agent_guard_loops],
              ['Stagnation detector', c.agent_guard_stagnations],
            ],
            true,
          )}
        </>
      )}
      {firedRows([
        ['Stream errors', c.stream_errors],
        ['Truncated at ceiling', c.truncated_generations],
        ['Dialect residue', c.dialect_residue],
        ['Unvalidatable schemas', c.unvalidatable_schemas],
        ['Normalization errors', c.normalization_errors],
      ])}
      {c.empty_responses > 0 && (
        <Row
          label="Empty responses"
          value={
            c.reasoning_only > 0
              ? `${formatCount(c.empty_responses)} (${formatCount(c.reasoning_only)} reasoning-only)`
              : formatCount(c.empty_responses)
          }
        />
      )}
      {observed.length > 0 && (
        <>
          <span className="text-xs font-semibold text-text-muted mt-xs">Observed</span>
          {observed}
        </>
      )}
    </div>
  );
};
