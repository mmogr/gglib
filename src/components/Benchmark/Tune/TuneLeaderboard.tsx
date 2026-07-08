/**
 * Sortable leaderboard of completed tune candidates: composite score, tool
 * accuracy, loop-avoidance rate, provenance, and a per-row "Apply" button
 * that writes the candidate's sampling settings to the model's
 * `inferenceDefaults`.
 *
 * @module components/Benchmark/Tune/TuneLeaderboard
 */

import { FC } from 'react';
import { Button } from '../../ui/Button';
import { cn } from '../../../utils/cn';
import type { TuneCandidateResult } from '../../../types/benchmark';

interface TuneLeaderboardProps {
  results: TuneCandidateResult[];
  onApply: (result: TuneCandidateResult) => void;
  applyingIndex?: number | null;
}

function sourceLabel(source: TuneCandidateResult['source']): string {
  switch (source.kind) {
    case 'user_grid':
      return 'sweep';
    case 'gguf_author_default':
      return 'gguf default';
    case 'family_preset':
      return source.family;
    default:
      return '—';
  }
}

function averageToolMatch(result: TuneCandidateResult): number {
  if (result.task_results.length === 0) return 0;
  const sum = result.task_results.reduce((acc, r) => acc + r.tool_match_score, 0);
  return sum / result.task_results.length;
}

function loopFreeRate(result: TuneCandidateResult): number {
  if (result.task_results.length === 0) return 0;
  const loopFree = result.task_results.filter(
    r => !r.loop_detected && !r.stagnation_detected,
  ).length;
  return loopFree / result.task_results.length;
}

export const TuneLeaderboard: FC<TuneLeaderboardProps> = ({
  results,
  onApply,
  applyingIndex,
}) => {
  const sorted = [...results].sort((a, b) => b.composite_score - a.composite_score);

  if (sorted.length === 0) {
    return <p className="text-xs text-text-muted p-base">No completed candidates yet.</p>;
  }

  return (
    <table className="w-full text-xs border-collapse">
      <thead className="sticky top-0 bg-background z-10">
        <tr className="text-left text-text-muted border-b border-border">
          <th className="px-base py-xs font-medium">#</th>
          <th className="px-base py-xs font-medium">Source</th>
          <th className="px-base py-xs font-medium">Temp</th>
          <th className="px-base py-xs font-medium">top_p</th>
          <th className="px-base py-xs font-medium">Score</th>
          <th className="px-base py-xs font-medium">Tool Acc.</th>
          <th className="px-base py-xs font-medium">Loop-free</th>
          <th className="px-base py-xs font-medium">Status</th>
          <th className="px-base py-xs font-medium" />
        </tr>
      </thead>
      <tbody>
        {sorted.map((result, i) => (
          <tr
            key={i}
            className="border-b border-border-light hover:bg-surface-elevated transition-colors"
          >
            <td className="px-base py-xs text-text-secondary">{i + 1}</td>
            <td className="px-base py-xs text-text-secondary">{sourceLabel(result.source)}</td>
            <td className="px-base py-xs text-text-secondary">
              {result.config.temperature?.toFixed(2) ?? '—'}
            </td>
            <td className="px-base py-xs text-text-secondary">
              {result.config.topP?.toFixed(2) ?? '—'}
            </td>
            <td className="px-base py-xs font-semibold text-text">
              {result.composite_score.toFixed(3)}
            </td>
            <td className="px-base py-xs text-text-secondary">
              {(averageToolMatch(result) * 100).toFixed(0)}%
            </td>
            <td className="px-base py-xs text-text-secondary">
              {(loopFreeRate(result) * 100).toFixed(0)}%
            </td>
            <td className="px-base py-xs">
              {result.pruned ? (
                <span className="py-xs px-sm rounded-sm font-medium bg-warning-subtle text-warning">
                  pruned
                </span>
              ) : (
                <span className="py-xs px-sm rounded-sm font-medium bg-success-subtle text-success">
                  survived
                </span>
              )}
            </td>
            <td className={cn('px-base py-xs')}>
              <Button
                variant="secondary"
                size="sm"
                disabled={applyingIndex === i}
                onClick={() => onApply(result)}
              >
                {applyingIndex === i ? 'Applying…' : 'Apply'}
              </Button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
};

export default TuneLeaderboard;
