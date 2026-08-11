import type { FC } from 'react';
import { Download } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { cn } from '../../../utils/cn';
import { formatMs, formatTps } from '../format';
import { AgenticReportVerdicts } from './AgenticReportVerdicts';
import { AgenticTaskDrilldown } from './AgenticTaskDrilldown';
import type { AgenticEvalReport } from '../../../types/benchmark';

const score = (v: number | null | undefined) => (v == null ? '—' : v.toFixed(3));
const signed = (v: number | null | undefined) =>
  v == null ? '—' : `${v > 0 ? '+' : ''}${v.toFixed(3)}`;
const factor = (v: number | null | undefined) => (v == null ? '—' : `${v.toFixed(2)}×`);

const Td: FC<{ children: React.ReactNode; className?: string }> = ({ children, className }) => (
  <td className={cn('px-md py-xs text-sm text-text font-mono tabular-nums', className)}>{children}</td>
);
const Th: FC<{ children?: React.ReactNode }> = ({ children }) => (
  <th className="px-md py-xs text-left text-xs font-medium text-text-muted">{children}</th>
);

const deltaClass = (v: number | null | undefined) =>
  v == null ? '' : v > 0 ? 'text-success' : v < 0 ? 'text-danger' : '';
const factorClass = (v: number | null | undefined) =>
  v == null ? '' : v > 1 ? 'text-success' : v < 1 ? 'text-danger' : '';

/**
 * The complete raw-vs-gglib report. Block order mirrors the CLI's renderer so
 * the two surfaces tell the same story: identity, sample size, the axis
 * table, validity verdicts, stability, efficiency, then the GUI-only
 * per-task drill-down.
 */
export const AgenticReport: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const seedCount = report.seeds?.length ?? 0;
  const loopDenominatorNote =
    report.raw.loop_avoidance == null || report.gglib.loop_avoidance == null
      ? `loop avoidance measured on ${report.raw.loop_eligible} raw and ${report.gglib.loop_eligible} gglib runs`
      : null;

  const download = () => {
    // The CLI's export shape (--output) with explicit nulls where the browser
    // has no provenance to offer — consumers of either file see the same keys.
    const payload = { gglib_version: null, hardware: null, report };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `agentic-eval-${report.model_name.replace(/[^\w.-]+/g, '_')}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const axisRows: Array<[string, number | null | undefined, number | null | undefined, number | null | undefined]> = [
    ['Tool accuracy', report.raw.tool_accuracy, report.gglib.tool_accuracy, report.delta.tool_accuracy],
    ['Loop avoidance', report.raw.loop_avoidance, report.gglib.loop_avoidance, report.delta.loop_avoidance],
    ['Task completion', report.raw.task_completion, report.gglib.task_completion, report.delta.task_completion],
    ['Composite', report.raw.composite, report.gglib.composite, report.delta.composite],
  ];

  const efficiencyRows: Array<[string, string, string, number | null | undefined]> = [
    ['Suite wall time', formatMs(report.raw.total_wall_ms), formatMs(report.gglib.total_wall_ms), report.delta.wall_time_speedup],
    [
      'Completion tokens',
      report.raw.total_completion_tokens?.toLocaleString() ?? '—',
      report.gglib.total_completion_tokens?.toLocaleString() ?? '—',
      report.delta.completion_token_ratio,
    ],
    ['1st tool call', formatMs(report.raw.mean_time_to_first_tool_call_ms), formatMs(report.gglib.mean_time_to_first_tool_call_ms), null],
    ['Throughput', formatTps(report.raw.tg_tps), formatTps(report.gglib.tg_tps), null],
  ];

  return (
    <div className="bg-surface rounded-md p-base flex flex-col gap-md">
      <div className="flex items-start justify-between gap-md">
        <div>
          <h2 className="m-0 text-lg font-semibold text-text">{report.model_name}</h2>
          <p className="m-0 text-xs text-text-muted font-mono tabular-nums">
            {report.param_count_b}B{report.quantization ? ` · ${report.quantization}` : ''} ·{' '}
            {report.ctx_size.toLocaleString()} ctx
          </p>
        </div>
        <Button variant="secondary" size="sm" onClick={download} leftIcon={<Icon icon={Download} size={14} />}>
          Download JSON
        </Button>
      </div>

      {seedCount >= 2 ? (
        <p className="m-0 text-xs text-text-muted">
          Scores are means of {seedCount} seeded runs per task{' '}
          <span className="font-mono tabular-nums">({report.seeds!.join(', ')})</span>.
        </p>
      ) : (
        <p className="m-0 text-xs text-warning">
          Single sample per task — the figures carry full decode variance. Re-run with two or more
          seeds before reading deltas as findings.
        </p>
      )}

      <div className="overflow-x-auto bg-surface-elevated rounded-md">
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-border-light">
              <Th />
              <Th>raw</Th>
              <Th>gglib</Th>
              <Th>delta</Th>
            </tr>
          </thead>
          <tbody>
            {axisRows.map(([label, raw, gglib, delta]) => (
              <tr key={label} className="border-b border-border-light last:border-b-0">
                <td className="px-md py-xs text-sm text-text-muted">{label}</td>
                <Td>{score(raw)}</Td>
                <Td>{score(gglib)}</Td>
                <Td className={deltaClass(delta)}>{signed(delta)}</Td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {loopDenominatorNote && <p className="m-0 text-xs text-text-muted">{loopDenominatorNote}</p>}

      <AgenticReportVerdicts report={report} />

      <section className="flex flex-col gap-xs">
        <h3 className="m-0 text-sm font-semibold text-text">Efficiency</h3>
        <div className="overflow-x-auto bg-surface-elevated rounded-md">
          <table className="w-full border-collapse">
            <thead>
              <tr className="border-b border-border-light">
                <Th />
                <Th>raw</Th>
                <Th>gglib</Th>
                <Th>factor</Th>
              </tr>
            </thead>
            <tbody>
              {efficiencyRows.map(([label, raw, gglib, fac]) => (
                <tr key={label} className="border-b border-border-light last:border-b-0">
                  <td className="px-md py-xs text-sm text-text-muted">{label}</td>
                  <Td>{raw}</Td>
                  <Td>{gglib}</Td>
                  <Td className={factorClass(fac)}>{factor(fac)}</Td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <AgenticTaskDrilldown report={report} />
    </div>
  );
};
