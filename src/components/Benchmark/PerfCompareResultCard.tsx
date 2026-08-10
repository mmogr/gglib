import type { FC } from 'react';
import { Zap } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { Readout } from '../primitives';
import { cn } from '../../utils/cn';
import { formatMs, formatTps } from './format';
import type {
  BenchmarkModelResult,
  ModelCompareResult,
  ModelPerfResult,
} from '../../types/benchmark';

export interface ModelResultState {
  modelId: number;
  modelName: string;
  status: 'pending' | 'running' | 'complete' | 'failed';
  /** Throttle-buffered text delta accumulation. */
  liveText: string;
  result?: BenchmarkModelResult;
  error?: string;
}

const CompareResult: FC<{ result: ModelCompareResult }> = ({ result: r }) => (
  <div className="flex flex-col gap-sm">
    <div className="bg-surface-elevated rounded-md p-base text-sm text-text whitespace-pre-wrap font-mono leading-relaxed">
      {r.response_text}
    </div>
    <div className="flex gap-md text-xs text-text-muted flex-wrap font-mono tabular-nums">
      {r.generation_tps != null && (
        <span className="inline-flex items-center gap-xs">
          <Icon icon={Zap} size={12} />
          {formatTps(r.generation_tps)} gen
        </span>
      )}
      {r.prompt_tps != null && <span>{formatTps(r.prompt_tps)} pp</span>}
      {r.generation_ms != null && <span>{formatMs(r.generation_ms)} gen</span>}
      {r.completion_tokens != null && <span>{r.completion_tokens} tokens</span>}
      {r.was_truncated && <span className="text-warning font-sans">truncated</span>}
    </div>
  </div>
);

const PerfResult: FC<{ result: ModelPerfResult }> = ({ result: r }) => (
  <div className="flex gap-lg flex-wrap text-sm">
    <div className="bg-surface-elevated rounded-md p-md min-w-[90px]">
      <Readout label="TG speed" value={r.tg_tps.toFixed(1)} unit="t/s" size="lg" align="center" />
    </div>
    <div className="bg-surface-elevated rounded-md p-md min-w-[90px]">
      <Readout label="PP speed" value={r.pp_tps.toFixed(1)} unit="t/s" size="lg" align="center" />
    </div>
    <div className="bg-surface-elevated rounded-md p-md min-w-[90px]">
      <Readout label="Backend" value={r.backend ?? '—'} size="sm" align="center" />
    </div>
    <div className="bg-surface-elevated rounded-md p-md min-w-[90px]">
      <Readout label="Reps" value={r.repetitions} size="sm" align="center" />
    </div>
  </div>
);

interface PerfCompareResultCardProps {
  model: ModelResultState;
  /** Live text only renders for compare mode — perf has no token stream. */
  showLiveText: boolean;
}

/** One model's card in the perf/compare results column. */
export const PerfCompareResultCard: FC<PerfCompareResultCardProps> = ({ model: m, showLiveText }) => {
  const statusColor = {
    pending: 'text-text-muted',
    running: 'text-primary',
    complete: 'text-text-muted',
    failed: 'text-danger',
  }[m.status];

  const statusLabel = {
    pending: 'Pending',
    running: 'Running…',
    complete: 'Complete',
    failed: 'Failed',
  }[m.status];

  return (
    <div className="bg-surface rounded-md p-base flex flex-col gap-sm">
      <div className="flex items-center gap-sm">
        <span className="font-medium text-text truncate flex-1">{m.modelName}</span>
        <span className={cn('text-xs font-medium', statusColor)}>{statusLabel}</span>
      </div>

      {m.status === 'running' && m.liveText && showLiveText && (
        <div className="bg-surface-elevated rounded-md p-base text-sm text-text whitespace-pre-wrap font-mono leading-relaxed max-h-[300px] overflow-y-auto">
          {m.liveText}
          <span className="inline-block w-2 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom" />
        </div>
      )}

      {m.status === 'complete' && m.result && (
        <>
          {m.result.kind === 'compare' && <CompareResult result={m.result} />}
          {m.result.kind === 'perf' && <PerfResult result={m.result} />}
        </>
      )}

      {m.status === 'failed' && m.error && (
        <div className="text-sm text-danger bg-danger-subtle rounded-md p-sm">{m.error}</div>
      )}
    </div>
  );
};
