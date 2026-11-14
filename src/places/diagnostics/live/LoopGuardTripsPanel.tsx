/**
 * The loop guard's log, on Diagnostics › Live — the GUI face of
 * `gglib proxy trips`.
 *
 * Per UTC day, model and gglib version: how many requests the guard
 * scanned, how many it acted on, by detector, and from how many sessions —
 * the same columns the CLI prints. A day scanned without a trip is a row with
 * zero trips, which is the reading ADR 0011's kill criterion is about. Read
 * from the daemon's `GET /api/proxy/loop-guard-trips`, which reads the
 * database, so it survives a restart. It shows the last
 * [`LOOP_GUARD_LOG_DAYS`] days; `gglib proxy trips --since 90` reads all the
 * log keeps.
 */

import { FC } from 'react';
import { Button } from '../../../components/ui/Button';
import { Banner } from '../../../components/ui/Banner';
import { LOOP_GUARD_LOG_DAYS, type LoopGuardTripsState } from './useLoopGuardTrips';

const COLUMNS = [
  'Day (UTC)',
  'Model',
  'Version',
  'Scanned',
  'Trips',
  'Loops',
  'Stagnation',
  'Sessions',
];

/**
 * The log as `useLoopGuardTrips` holds it. The Live tab draws it only once
 * it holds a day or an error, so an empty log is never drawn here.
 */
export const LoopGuardTripsPanel: FC<{ state: LoopGuardTripsState }> = ({ state }) => {
  const { days, loading, error, reload } = state;

  // A failed read, until a read lands; Retry's read in flight says so.
  if (error && !loading) {
    return (
      <Banner
        variant="danger"
        action={
          <Button type="button" size="sm" variant="secondary" onClick={() => void reload()}>
            Retry
          </Button>
        }
      >
        {error}
      </Banner>
    );
  }

  if (days === null) {
    return <p className="text-xs text-text-muted m-0">Reading the loop guard&apos;s log…</p>;
  }

  return (
    <section aria-label="Loop guard log">
      <div className="flex items-center justify-between gap-md mb-xs">
        <h4 className="m-0 text-xs font-semibold text-text">What it did, the last {LOOP_GUARD_LOG_DAYS} days</h4>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          isLoading={loading}
          onClick={() => void reload()}
        >
          Refresh
        </Button>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-xs border-collapse">
          <thead>
            <tr className="text-left text-text-muted border-b border-border">
              {COLUMNS.map((column) => (
                <th key={column} className="px-xs py-2xs font-semibold">
                  {column}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {days.map((d) => (
              <tr
                key={`${d.day}|${d.model_name}|${d.gglib_version}`}
                className="border-b border-border-light"
              >
                <td className="px-xs py-2xs font-mono tabular-nums">{d.day}</td>
                <td className="px-xs py-2xs font-mono">{d.model_name}</td>
                <td className="px-xs py-2xs font-mono">{d.gglib_version}</td>
                <td className="px-xs py-2xs tabular-nums">{d.scanned}</td>
                <td className="px-xs py-2xs tabular-nums">{d.trips}</td>
                <td className="px-xs py-2xs tabular-nums">{d.loops}</td>
                <td className="px-xs py-2xs tabular-nums">{d.stagnations}</td>
                <td className="px-xs py-2xs tabular-nums">{d.sessions}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="m-0 mt-xs text-xs text-text-muted">
        A trip is the guard&apos;s decision, not a delivery: a noted request can still fail to
        reach the model.
      </p>
    </section>
  );
};
