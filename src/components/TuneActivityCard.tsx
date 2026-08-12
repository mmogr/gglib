/**
 * Recent tune activity — what the gate decided, made visible.
 *
 * Refusals render beside applies deliberately: "gglib checked and chose not
 * to change anything" is the sentence that builds trust, and a feed showing
 * only applies would look like a system that always meddles.
 *
 * Sourced from the benchmark run rows, which carry the gate's `ApplyRecord`
 * (written for refusals too).
 *
 * This used to filter on an `initiator` slug that the idle-time scheduler
 * stamped into the stored `TuneConfig`, showing only runs the scheduler
 * started. With the scheduler gone nothing stamps one, so that filter would
 * have emptied the card permanently. It is inverted: every tune run appears,
 * which is now every tune run there is.
 */

import { FC, useEffect, useState } from 'react';
import type { ApplyVerdict, BenchmarkRun } from '../types/benchmark';
import { listBenchmarkRuns } from '../services/clients/benchmark';

/** How many recent runs to scan. */
const SCAN_LIMIT = 50;

/** The feed's own cap: enough to show a pattern, not a log file. */
const SHOW_LIMIT = 8;

interface ActivityEntry {
  runId: number;
  startedAt: string;
  verdict: ApplyVerdict | null;
  status: BenchmarkRun['status'];
}

function parseEntry(run: BenchmarkRun): ActivityEntry | null {
  if (run.run_type !== 'tune' || !run.config_json) return null;

  let verdict: ApplyVerdict | null = null;
  if (run.applied_json) {
    try {
      verdict = (JSON.parse(run.applied_json) as { verdict: ApplyVerdict }).verdict;
    } catch {
      verdict = null;
    }
  }
  return {
    runId: run.id,
    startedAt: run.created_at,
    verdict,
    status: run.status,
  };
}

function describeVerdict(verdict: ApplyVerdict | null, status: BenchmarkRun['status']): {
  text: string;
  applied: boolean;
} {
  if (verdict == null) {
    return {
      text: status === 'running' ? 'running…' : 'no verdict recorded',
      applied: false,
    };
  }
  switch (verdict.verdict) {
    case 'apply':
      return {
        text: `applied — margin ${verdict.margin >= 0 ? '+' : ''}${verdict.margin.toFixed(3)} vs drift ${verdict.drift.toFixed(3)}`,
        applied: true,
      };
    case 'incumbent_stands':
      return {
        text: `refused — incumbent stands at ${verdict.incumbent_mean.toFixed(3)}`,
        applied: false,
      };
    case 'within_drift':
      return {
        text: `refused — margin ${verdict.margin >= 0 ? '+' : ''}${verdict.margin.toFixed(3)} inside drift ${verdict.drift.toFixed(3)}`,
        applied: false,
      };
    case 'paired_disagrees':
      return {
        text: `refused — pairs disagree (${verdict.wins}W–${verdict.losses}L)`,
        applied: false,
      };
    case 'uncalibrated':
      return { text: 'refused — uncalibrated run', applied: false };
    case 'contaminated':
      return {
        text: `refused — ${verdict.unmeasured_runs} unmeasured run(s)`,
        applied: false,
      };
    default:
      return { text: 'unrecognised verdict', applied: false };
  }
}

export const TuneActivityCard: FC = () => {
  const [entries, setEntries] = useState<ActivityEntry[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    listBenchmarkRuns(SCAN_LIMIT, 0)
      .then((runs) => {
        if (cancelled) return;
        setEntries(
          runs
            .map(parseEntry)
            .filter((e): e is ActivityEntry => e != null)
            .slice(0, SHOW_LIMIT),
        );
      })
      .catch(() => {
        if (!cancelled) setEntries([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (entries == null) {
    return <p className="text-xs text-text-muted m-0">Loading…</p>;
  }
  if (entries.length === 0) {
    return (
      <p className="text-xs text-text-muted m-0 leading-relaxed">
        Nothing yet. Run a tune and its decision lands here — applies and
        refusals alike, each with the numbers that decided it.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-1.5">
      {entries.map((entry) => {
        const { text, applied } = describeVerdict(entry.verdict, entry.status);
        return (
          <div
            key={entry.runId}
            className="flex items-baseline gap-2 text-xs font-mono tabular-nums"
          >
            <span className="text-text-muted shrink-0">
              {entry.startedAt.slice(0, 16).replace('T', ' ')}
            </span>
            <span className={applied ? 'text-success' : 'text-text'}>{text}</span>
            <span className="text-text-muted ml-auto shrink-0">run #{entry.runId}</span>
          </div>
        );
      })}
    </div>
  );
};

export default TuneActivityCard;
