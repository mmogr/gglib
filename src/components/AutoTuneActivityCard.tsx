/**
 * Recent auto-tune activity — the loop's decisions, made visible.
 *
 * Refusals render beside applies deliberately: "gglib checked and chose not
 * to change anything" is the sentence that builds trust in autonomy, and a
 * feed showing only applies would look like a system that always meddles.
 *
 * Sourced from the benchmark run rows, which already carry everything: the
 * initiator slug the scheduler stamps into the stored `TuneConfig`, and the
 * gate's `ApplyRecord` (written for refusals too). Runs without an initiator
 * are the operator's own and stay out of this feed — they live in the
 * benchmark tab's history where they always did.
 */

import { FC, useEffect, useState } from 'react';
import type { ApplyVerdict, BenchmarkRun } from '../types/benchmark';
import { listBenchmarkRuns } from '../services/clients/benchmark';

/** How many recent runs to scan for auto-initiated ones. */
const SCAN_LIMIT = 50;

/** The feed's own cap: enough to show a pattern, not a log file. */
const SHOW_LIMIT = 8;

interface ActivityEntry {
  runId: number;
  startedAt: string;
  initiator: string;
  verdict: ApplyVerdict | null;
  status: BenchmarkRun['status'];
}

function parseEntry(run: BenchmarkRun): ActivityEntry | null {
  if (run.run_type !== 'tune' || !run.config_json) return null;
  let initiator: string | null = null;
  try {
    const config = JSON.parse(run.config_json) as { initiator?: string | null };
    initiator = config.initiator ?? null;
  } catch {
    return null;
  }
  if (initiator == null) return null;

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
    initiator,
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

function initiatorLabel(slug: string): string {
  if (slug === 'idle') return 'idle tune';
  if (slug.startsWith('signal:')) return `signal (${slug.slice('signal:'.length)})`;
  return slug;
}

export const AutoTuneActivityCard: FC = () => {
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
        Nothing yet. When idle-time auto-tune runs, its decisions land here —
        applies and refusals alike, each with the numbers that decided it.
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
            <span className="text-text-secondary shrink-0">
              {initiatorLabel(entry.initiator)}
            </span>
            <span className={applied ? 'text-success' : 'text-text'}>{text}</span>
            <span className="text-text-muted ml-auto shrink-0">run #{entry.runId}</span>
          </div>
        );
      })}
    </div>
  );
};

export default AutoTuneActivityCard;
