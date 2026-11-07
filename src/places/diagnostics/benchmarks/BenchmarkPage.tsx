/**
 * Diagnostics › Benchmarks.
 *
 * Mode tabs + per-mode dispatch + the shared run-history strip, read when
 * the tab opens and again when a run completes.
 * Each mode owns its own state and streaming:
 * - Perf / Compare: `PerfCompareTab`
 * - Tune: `Tune/TuneTab`
 * - Agentic: `Agentic/AgenticTab`
 *
 * @module places/diagnostics/benchmarks/BenchmarkPage
 */

import { FC, useCallback, useEffect, useState } from 'react';
import { Tabs } from '../../../components/ui/Tabs';
import { PerfCompareTab } from './PerfCompareTab';
import { RunHistoryTable } from './RunHistoryTable';
import { TuneTab } from './Tune/TuneTab';
import { AgenticTab } from './Agentic/AgenticTab';
import type { GgufModel } from '../../../types';
import type { BenchmarkRun } from '../../../types/benchmark';
import { listBenchmarkRuns } from '../../../services/clients/benchmark';

type RunMode = 'compare' | 'perf' | 'tune' | 'agentic';

interface BenchmarkPageProps {
  /** All available models for selection. */
  models: GgufModel[];
  /** Pre-selected model IDs to benchmark (optional, user can change). */
  initialModelIds?: number[];
}

const BenchmarkPage: FC<BenchmarkPageProps> = ({ models, initialModelIds }) => {
  const [mode, setMode] = useState<RunMode>('perf');
  const [history, setHistory] = useState<BenchmarkRun[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);

  const loadHistory = useCallback(async () => {
    setHistoryLoading(true);
    try {
      setHistory(await listBenchmarkRuns(20, 0));
    } catch {
      // non-fatal; history may be unavailable during an active run
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  const refreshHistory = useCallback(() => void loadHistory(), [loadHistory]);

  return (
    <div className="flex flex-col h-full w-full bg-background overflow-hidden">
      <header className="flex items-center gap-md px-base py-sm border-b border-border-light shrink-0">
        <Tabs<RunMode>
          aria-label="Benchmark mode"
          size="sm"
          divider={false}
          activeId={mode}
          onChange={setMode}
          tabs={[
            { id: 'perf', label: 'Perf' },
            { id: 'compare', label: 'Compare' },
            { id: 'tune', label: 'Tune' },
            { id: 'agentic', label: 'Agentic' },
          ]}
        />
      </header>

      {mode === 'tune' && <TuneTab models={models} onRunComplete={refreshHistory} />}
      {mode === 'agentic' && <AgenticTab models={models} onRunComplete={refreshHistory} />}
      {/* Kept mounted while other tabs are open, matching the old page: an
          in-flight perf/compare run keeps streaming and form state survives
          mode switches. */}
      <PerfCompareTab
        mode={mode === 'compare' ? 'compare' : 'perf'}
        active={mode === 'perf' || mode === 'compare'}
        models={models}
        initialModelIds={initialModelIds}
        onRunComplete={refreshHistory}
      />

      <RunHistoryTable history={history} loading={historyLoading} />
    </div>
  );
};

export default BenchmarkPage;
