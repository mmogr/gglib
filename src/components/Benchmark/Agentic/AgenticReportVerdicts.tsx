import type { FC } from 'react';
import { Banner } from '../../ui/Banner';
import type { AgenticEvalReport, ArmScores, EvalArm } from '../../../types/benchmark';
import {
  EFFECT_NOISE_RATIO,
  controlVerdict,
  effectVerdict,
  passCounts,
  unstableTasks,
} from './verdicts';

const fmt = (v: number) => v.toFixed(3);

/** Arms with runs the harness never delivered to the model — means are floors, not measurements. */
const UnmeasuredBlock: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const arms: Array<[EvalArm, ArmScores | null | undefined]> = [
    ['raw', report.raw],
    ['gglib', report.gglib],
    ['raw_replicate', report.raw_replicate],
    ['control', report.control],
  ];
  const partly = arms.filter(([, a]) => a && (a.unmeasured_runs ?? 0) > 0);
  if (partly.length === 0) return null;

  return (
    <Banner variant="danger" title="Unmeasured runs">
      {partly.map(([arm, a]) => (
        <p key={arm} className="m-0 font-mono tabular-nums">
          {arm}: {a!.unmeasured_runs}/{a!.runs ?? '?'} runs unmeasured
        </p>
      ))}
      <p className="m-0">Read those arms as a floor, not a measurement.</p>
    </Banner>
  );
};

/** The A/A drift check: is the measured effect bigger than the eval's own noise? */
const NoiseBlock: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const verdict = effectVerdict(report);
  if (verdict == null) {
    return (
      <p className="text-xs text-text-muted m-0">
        No A/A arm ran — read the composite delta as a direction, not a magnitude.
      </p>
    );
  }
  const exceeded = verdict.kind === 'exceeds_noise';
  return (
    <div className="flex flex-col gap-xs">
      <p className={`text-sm m-0 ${exceeded ? 'text-text' : 'text-warning'}`}>
        {exceeded ? 'Effect exceeds the eval’s own drift' : 'Effect is within the eval’s own drift'}
        <span className="font-mono tabular-nums text-text-secondary">
          {' '}
          — effect {fmt(Math.abs(verdict.effect))}, drift {fmt(verdict.noise)} (bar:{' '}
          {EFFECT_NOISE_RATIO.toFixed(1)}×)
        </span>
      </p>
      <p className="text-xs text-text-muted m-0">
        A sanity ratio, not a significance test — more seeds strengthen it, a bigger factor does not.
        {verdict.pairs > 1 && ` Drift is the mean of ${verdict.pairs} pairwise gaps.`}
      </p>
    </div>
  );
};

/** The paired per-(task, seed) view, when the report carries one. */
const PairedBlock: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const paired = report.paired;
  if (paired == null) return null;
  const p =
    paired.p_value == null
      ? 'too few non-tied pairs for a p — read wins against losses'
      : `Wilcoxon one-sided p = ${paired.p_value.toFixed(4)}`;
  return (
    <div className="flex flex-col gap-xs">
      <p className="text-sm text-text m-0">
        Paired
        <span className="font-mono tabular-nums text-text-secondary">
          {' '}
          — {paired.wins}W–{paired.losses}L–{paired.ties}T over {paired.pairs} pairs, mean Δ{' '}
          {paired.mean_delta >= 0 ? '+' : ''}
          {paired.mean_delta.toFixed(3)}; {p}
        </span>
      </p>
      {paired.unmeasured_pairs > 0 && (
        <p className="text-xs text-warning m-0">
          {paired.unmeasured_pairs} pairs dropped: at least one side never reached the model.
        </p>
      )}
    </div>
  );
};

/** What the positive control demonstrated about this run's sensitivity. */
const ControlBlock: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const verdict = controlVerdict(report);
  if (verdict == null) {
    return (
      <p className="text-xs text-text-muted m-0">
        No control arm ran — nothing was demonstrated about sensitivity either way.
      </p>
    );
  }

  const gapText = (
    <span className="font-mono tabular-nums text-text-secondary">
      {' '}
      — gap {fmt(verdict.gap)}, gglib {fmt(report.gglib.composite)} vs control{' '}
      {fmt(report.control!.composite)}
    </span>
  );

  return (
    <div className="flex flex-col gap-xs">
      {verdict.kind === 'moved' && (
        <p className="text-sm text-text m-0">Control moved — the eval detects broken sampling{gapText}</p>
      )}
      {verdict.kind === 'too_small' && (
        <Banner variant="danger" title="Control barely moved">
          Deliberately broken sampling scored within {fmt(verdict.gap)} of the real pipeline —
          treat every delta in this report as uninterpretable.
        </Banner>
      )}
      {verdict.kind === 'wrong_direction' && (
        <Banner variant="danger" title="Control moved the wrong way">
          Broken sampling scored above the real pipeline by {fmt(verdict.gap)} — this run cannot
          support any conclusion.
        </Banner>
      )}
      {(report.control?.seeds ?? 0) > 0 &&
        (report.seeds?.length ?? 0) > (report.control?.seeds ?? 0) && (
          <p className="text-xs text-text-muted m-0">
            The control repeated {report.control?.seeds} of {report.seeds?.length} seeds.
          </p>
        )}
      <p className="text-xs text-text-muted m-0">
        Sensitivity at this gap says nothing about resolving a smaller effect.
      </p>
    </div>
  );
};

/** Per-seed stability: tasks that disagreed with themselves across seeds. */
const StabilityBlock: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const seedCount = report.seeds?.length ?? 0;
  if (seedCount < 2) return null;
  const unstable = unstableTasks(report);

  if (unstable.length === 0) {
    return (
      <p className="text-xs text-text-muted m-0">
        Every task returned the same verdict on all {seedCount} seeds in both arms.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-xs">
      <p className="text-sm text-warning m-0">
        {unstable.length} task{unstable.length === 1 ? '' : 's'} flipped between seeds:
      </p>
      {unstable.map((t) => {
        const [rawPassed, gglibPassed] = passCounts(t);
        return (
          <p key={t.task_id} className="text-xs text-text-secondary m-0 font-mono tabular-nums">
            {t.task_id} — raw {rawPassed}/{t.raw.length}, gglib {gglibPassed}/{t.gglib.length} seeds passed
          </p>
        );
      })}
    </div>
  );
};

/** The report's validity story, in the CLI's order: unmeasured, drift, control, stability. */
export const AgenticReportVerdicts: FC<{ report: AgenticEvalReport }> = ({ report }) => (
  <div className="flex flex-col gap-md">
    <UnmeasuredBlock report={report} />
    <section className="flex flex-col gap-xs">
      <h3 className="m-0 text-sm font-semibold text-text">Drift (A/A)</h3>
      <NoiseBlock report={report} />
      <PairedBlock report={report} />
    </section>
    <section className="flex flex-col gap-xs">
      <h3 className="m-0 text-sm font-semibold text-text">Positive control</h3>
      <ControlBlock report={report} />
    </section>
    <section className="flex flex-col gap-xs">
      <h3 className="m-0 text-sm font-semibold text-text">Stability</h3>
      <StabilityBlock report={report} />
    </section>
  </div>
);
