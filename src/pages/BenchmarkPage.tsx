/**
 * Benchmark Dashboard page.
 *
 * Header + mode tabs + per-mode dispatch + the shared run-history strip.
 * Each mode owns its own state and streaming:
 * - Perf / Compare: `components/Benchmark/PerfCompareTab`
 * - Tune: `components/Benchmark/Tune/TuneTab`
 * - Agentic: `components/Benchmark/Agentic/AgenticTab`
 *
 * @module pages/BenchmarkPage
 */

import { FC, useCallback, useEffect, useState } from 'react';
import { ArrowLeft, BarChart2 } from 'lucide-react';
import { Tabs } from '../components/ui/Tabs';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { PerfCompareTab } from '../components/Benchmark/PerfCompareTab';
import { RunHistoryTable } from '../components/Benchmark/RunHistoryTable';
import { TuneTab } from '../components/Benchmark/Tune/TuneTab';
import { AgenticTab } from '../components/Benchmark/Agentic/AgenticTab';
import type { GgufModel } from '../types';
import type { BenchmarkRun } from '../types/benchmark';
import { listBenchmarkRuns } from '../services/clients/benchmark';

type RunMode = 'compare' | 'perf' | 'tune' | 'agentic';

interface BenchmarkPageProps {
  /** All available models for selection. */
  models: GgufModel[];
  /** Pre-selected model IDs to benchmark (optional, user can change). */
  initialModelIds?: number[];
  onClose: () => void;
}

const BenchmarkPage: FC<BenchmarkPageProps> = ({ models, initialModelIds, onClose }) => {
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
        <Button variant="ghost" size="sm" iconOnly onClick={onClose} aria-label="Close benchmark">
          <Icon icon={ArrowLeft} size={16} />
        </Button>
        <Icon icon={BarChart2} size={18} className="text-primary" />
        <h1 className="text-base font-semibold text-text m-0 flex-1">Benchmark Dashboard</h1>
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

      <RunHistoryTable history={history} loading={historyLoading} onRefresh={refreshHistory} />
    </div>
  );
};

export default BenchmarkPage;
