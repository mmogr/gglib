/**
 * The Live tab's sections drawn straight from the frame: the requests (in
 * flight, then the recent ledger), the per-model signals, and llama.cpp's
 * slots as bars.
 *
 * @module places/diagnostics/live/ProxyMetricsGrid
 */

import type { FC } from 'react';
import type { DashboardSnapshot } from '../../../services/transport/types/dashboard';
import { recentRows } from '../../endpoint/lines';
import { ConnectionRow } from './ConnectionRow';
import { ModelSignalsCard, countOf, hasSignal } from './ModelSignalsCard';
import { slotContext, slotContextWords } from './slotContext';

interface SectionProps {
  /** The latest dashboard frame. */
  snapshot: DashboardSnapshot;
}

/**
 * The requests: each one in flight with its phase (and its prompt's bar
 * while it is read), then every request the proxy keeps, newest first, with
 * its tokens, its rate and what gglib did to it in the proxy's words.
 */
export const RequestsSection: FC<SectionProps> = ({ snapshot }) => {
  const connections = snapshot.active_connections;
  const rows = recentRows(snapshot, Infinity);
  return (
    <div className="flex flex-col gap-sm">
      {connections.length > 0 && (
        <div aria-label="In flight" role="list" className="flex flex-col gap-sm">
          {connections.map((connection) => (
            <div role="listitem" key={connection.id}>
              <ConnectionRow connection={connection} />
            </div>
          ))}
        </div>
      )}
      {rows.length > 0 && (
        <ul aria-label="Recent requests" className="m-0 flex list-none flex-col p-0">
          {rows.map((r) => (
            <li key={r.key} className="flex items-baseline gap-md py-xs text-xs text-text-secondary tabular-nums">
              <span className="shrink-0">{r.time}</span>
              <span className="min-w-0 truncate text-text">{r.model}</span>
              {r.figures && <span className="shrink-0">{r.figures}</span>}
              <span className="flex-1" />
              {r.done && <span className="shrink-0 text-text">{r.done}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

/**
 * llama.cpp's slots on the primary model: each a bar of the context it
 * holds, with the tokens and the percent. While llama.cpp reports none, the
 * proxy's reason for it.
 */
export const SlotsSection: FC<SectionProps> = ({ snapshot }) => {
  if (!snapshot.slots_available || snapshot.slots.length === 0) {
    return <p className="m-0 text-xs text-text-secondary">{snapshot.slots_status}</p>;
  }
  return (
    <ul aria-label="Slots" className="m-0 flex list-none flex-col gap-sm p-0">
      {snapshot.slots.map((slot) => {
        const context = slotContext(slot);
        return (
          <li key={slot.id} className="flex flex-col gap-xs text-xs text-text-secondary tabular-nums">
            <span className="flex justify-between gap-md">
              <span className="text-text">{`Slot ${slot.id}${slot.is_processing ? ' · active' : ''}`}</span>
              <span>{context ? slotContextWords(context) : 'context unknown'}</span>
            </span>
            {context && (
              <span
                role="progressbar"
                aria-label={`Slot ${slot.id} context`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={context.percent}
                className="block h-[2px] w-full overflow-hidden rounded-full bg-surface-elevated"
              >
                {/* Dynamic: the bar's fill is the slot's share of its context. */}
                <span className="block h-full bg-text-secondary" style={{ width: `${context.percent}%` }} />
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
};

/**
 * Per-model signals: what failed, what went in circles, and for which model —
 * the section `gglib proxy dashboard` prints, under the same rules (#1092).
 *
 * Only a model with something to report gets a card, since listing every
 * clean one would bury the one that is not. A clean run says so over its
 * denominator; Live draws this only once a model has been counted, so
 * "none" is always a claim over evidence. Cards come in name order, as the CLI's do: the proxy sends a hash map, and
 * its order can change from one frame to the next.
 */
export const ModelSignalsSection: FC<SectionProps> = ({ snapshot }) => {
  const perModel = Object.entries(snapshot.per_model_defects).sort(([a], [b]) =>
    a < b ? -1 : a > b ? 1 : 0,
  );
  const reporting = perModel.filter(([, counts]) => hasSignal(counts));
  // Agent turns are named beside requests, never added to them: one client
  // conversation is many agent turns, and a sum would be neither.
  const requests = perModel.reduce((sum, [, c]) => sum + c.requests, 0);
  const turns = perModel.reduce((sum, [, c]) => sum + c.agent_guard_scanned, 0);
  const across = `${countOf(requests, 'request')}${turns > 0 ? ` and ${countOf(turns, 'agent turn')}` : ''}`;

  return (
    <div>
      {reporting.length > 0 ? (
        <div className="flex flex-col gap-sm">
          {reporting.map(([model, counts]) => (
            <ModelSignalsCard key={model} model={model} counts={counts} />
          ))}
        </div>
      ) : (
        <p className="text-sm text-text-muted">{`None across ${across}, ${countOf(perModel.length, 'model')}.`}</p>
      )}
    </div>
  );
};
