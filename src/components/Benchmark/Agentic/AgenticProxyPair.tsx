import type { FC } from 'react';
import { cn } from '../../../utils/cn';
import type { AgenticEvalReport } from '../../../types/benchmark';

type Cell = number | null | undefined;

const score = (v: Cell) => (v == null ? '—' : v.toFixed(3));
const signed = (v: Cell) => (v == null ? '—' : `${v > 0 ? '+' : ''}${v.toFixed(3)}`);
const deltaClass = (v: Cell) =>
  v == null ? '' : v > 0 ? 'text-success' : v < 0 ? 'text-danger' : '';
const TH = 'px-md py-xs text-left text-xs font-medium text-text-muted';
const TD = 'px-md py-xs text-sm text-text font-mono tabular-nums';

/**
 * The proxy pair: the proxy arm beside its raw-auto baseline, and what the
 * proxy counted while the arm ran. Mirrors the CLI's "through gglib-proxy"
 * block.
 *
 * Both arms open with `tool_choice: "auto"`, under which the proxy judges
 * every call, so they are compared with each other and never with the
 * raw/gglib table above. The delta is everything the proxy does, its request
 * pipeline included. The repair counts are the only record of whether repair
 * did anything: a repaired call reaches the agent as the repaired call.
 */
export const AgenticProxyPair: FC<{ report: AgenticEvalReport }> = ({ report }) => {
  const pair = report.proxy;
  if (!pair) return null;
  const { raw_auto: rawAuto, proxy, delta, paired, defects, settings } = pair;
  const rows: Array<[string, Cell, Cell, Cell]> = [
    ['Tool accuracy', rawAuto.tool_accuracy, proxy.tool_accuracy, delta.tool_accuracy],
    ['Loop avoidance', rawAuto.loop_avoidance, proxy.loop_avoidance, delta.loop_avoidance],
    ['Task completion', rawAuto.task_completion, proxy.task_completion, delta.task_completion],
    ['Composite', rawAuto.composite, proxy.composite, delta.composite],
  ];
  // `delta_of` names its counts for raw and gglib; here they are raw (auto)
  // and the proxy arm.
  const withheld = delta.withheld;

  return (
    <section className="flex flex-col gap-sm" aria-label="Through gglib-proxy">
      <h3 className="m-0 text-sm font-semibold text-text">Through gglib-proxy</h3>
      <p className="m-0 text-xs text-text-muted">
        Both arms open with tool_choice &quot;auto&quot;, under which the proxy judges every call
        whose schema it can judge. Compare them with each other, not with the table above.
      </p>
      <table className="w-full border-collapse">
        <thead>
          <tr>
            <th className={TH}>Axis</th>
            <th className={TH}>Raw (auto)</th>
            <th className={TH}>Proxy</th>
            <th className={TH}>Δ</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([name, a, b, d]) => (
            <tr key={name}>
              <td className="px-md py-xs text-sm text-text">{name}</td>
              <td className={TD}>{score(a)}</td>
              <td className={TD}>{score(b)}</td>
              <td className={cn(TD, deltaClass(d))}>{signed(d)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {withheld && (
        <p className="m-0 text-xs text-warning">
          Delta withheld: {withheld.raw} raw (auto) and {withheld.gglib} proxy runs never reached
          the model, so a difference here would be partly a difference in failures.
        </p>
      )}
      <p className="m-0 text-xs text-text-muted">
        The delta is everything the proxy does, its request pipeline included; the counts below
        say whether repair was part of it.
      </p>
      {paired && (
        <p className="m-0 text-xs text-text-muted">
          Paired: the proxy scored higher on {paired.wins}, raw (auto) on {paired.losses}, and{' '}
          {paired.ties} tied, of {paired.pairs} pairs
          {paired.p_value != null ? `; one-sided p ${paired.p_value.toFixed(3)}` : ''}.
        </p>
      )}
      <p className="m-0 text-xs text-text">
        The proxy handled {defects.requests} request(s): {defects.repairs_attempted} repair(s)
        attempted, {defects.repairs_succeeded} succeeded; {defects.loop_guard_trips} loop-guard
        intervention(s).
      </p>
      {!settings.tool_call_repair && (
        <p className="m-0 text-xs text-warning">
          Repair was off (GGLIB_DISABLE_TOOL_REPAIR is set), so this pair measured the proxy
          without it.
        </p>
      )}
      {settings.tool_call_repair && defects.repairs_attempted === 0 && (
        <p className="m-0 text-xs text-text-muted">
          The proxy attempted no repair: no call it could judge broke its schema, so this pair
          says nothing about what repair changes.
        </p>
      )}
    </section>
  );
};
