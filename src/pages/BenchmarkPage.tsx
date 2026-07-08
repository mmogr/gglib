/**
 * Benchmark Dashboard page.
 *
 * Provides:
 * - Mode selector: "Compare" (side-by-side response quality) vs "Perf" (throughput)
 *   vs "Tune" (sampling-parameter sweep for agentic tool-calling accuracy)
 * - Config form: model multi-select, prompt (compare) or pp/tg/reps (perf)
 * - Live streaming results panel with 100 ms throttled text delta rendering
 * - History table of recent benchmark runs
 *
 * Throttle pattern: incoming `model_text_delta` events are buffered in a
 * `useRef<Map<number, string>>` and flushed into React state every 100 ms to
 * avoid per-keystroke re-renders during fast SSE streams. Tune mode
 * (`components/Benchmark/Tune/TuneTab`) reuses this exact pattern for its
 * own high-frequency `tune_task_complete` events — see that module's docs.
 *
 * @module pages/BenchmarkPage
 */

import {
  FC,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import { ArrowLeft, BarChart2, Play, Square, Target, Zap } from 'lucide-react';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { Input } from '../components/ui/Input';
import { Textarea } from '../components/ui/Textarea';
import { cn } from '../utils/cn';
import { TuneTab } from '../components/Benchmark/Tune/TuneTab';
import type { GgufModel } from '../types';
import type {
  BenchmarkEvent,
  BenchmarkModelResult,
  BenchmarkRun,
  CompareConfig,
  ModelCompareResult,
  ModelPerfResult,
  PerfConfig,
} from '../types/benchmark';
import {
  listBenchmarkRuns,
  startCompareRun,
  startPerfRun,
} from '../services/clients/benchmark';

// ─── Types ────────────────────────────────────────────────────────────────────

type RunMode = 'compare' | 'perf' | 'tune';

interface ModelResultState {
  modelId: number;
  modelName: string;
  status: 'pending' | 'running' | 'complete' | 'failed';
  liveText: string;  // throttle-buffered text delta accumulation
  result?: BenchmarkModelResult;
  error?: string;
}

interface RunState {
  runId?: number;
  status: 'idle' | 'running' | 'complete' | 'failed';
  error?: string;
  models: ModelResultState[];
}

// ─── Props ────────────────────────────────────────────────────────────────────

interface BenchmarkPageProps {
  /** All available models for selection. */
  models: GgufModel[];
  /** Pre-selected model IDs to benchmark (optional, user can change). */
  initialModelIds?: number[];
  onClose: () => void;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function formatTps(tps: number | null | undefined): string {
  if (tps == null) return '—';
  return `${tps.toFixed(1)} t/s`;
}

function formatMs(ms: number | null | undefined): string {
  if (ms == null) return '—';
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${ms.toFixed(0)} ms`;
}

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return iso;
  }
}

// ─── Component ────────────────────────────────────────────────────────────────

const BenchmarkPage: FC<BenchmarkPageProps> = ({ models, initialModelIds, onClose }) => {
  const [mode, setMode] = useState<RunMode>('perf');

  // Config form state
  const [selectedModelIds, setSelectedModelIds] = useState<number[]>(
    initialModelIds ?? (models.length > 0 && models[0].id != null ? [models[0].id] : []),
  );
  const [prompt, setPrompt] = useState('Tell me a short story about a robot.');
  const [systemPrompt, setSystemPrompt] = useState('');
  const [ctxSize, setCtxSize] = useState('');
  const [ppTokens, setPpTokens] = useState('512');
  const [tgTokens, setTgTokens] = useState('128');
  const [repetitions, setRepetitions] = useState('3');

  // Run state
  const [runState, setRunState] = useState<RunState>({ status: 'idle', models: [] });

  // History state
  const [history, setHistory] = useState<BenchmarkRun[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);

  // Abort controller for SSE cleanup
  const abortRef = useRef<AbortController | null>(null);

  // Throttle buffer: model_id → accumulated text not yet flushed to state
  const textBufferRef = useRef<Map<number, string>>(new Map());
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ─── History fetch ──────────────────────────────────────────────────────────

  const loadHistory = useCallback(async () => {
    setHistoryLoading(true);
    try {
      const runs = await listBenchmarkRuns(20, 0);
      setHistory(runs);
    } catch {
      // non-fatal; history may be unavailable during active run
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadHistory();
    return () => {
      // Cancel any in-flight SSE on unmount
      abortRef.current?.abort();
      if (flushTimerRef.current !== null) clearTimeout(flushTimerRef.current);
    };
  }, [loadHistory]);

  // ─── Throttled flush ────────────────────────────────────────────────────────

  const scheduleFlush = useCallback(() => {
    if (flushTimerRef.current !== null) return;
    flushTimerRef.current = setTimeout(() => {
      flushTimerRef.current = null;
      const snapshot = new Map(textBufferRef.current);
      textBufferRef.current = new Map();
      if (snapshot.size === 0) return;
      setRunState(prev => {
        const next = { ...prev, models: prev.models.map(m => {
          const buf = snapshot.get(m.modelId);
          return buf != null ? { ...m, liveText: m.liveText + buf } : m;
        }) };
        return next;
      });
    }, 100);
  }, []);

  // ─── Event handler ──────────────────────────────────────────────────────────

  const handleEvent = useCallback((event: BenchmarkEvent) => {
    switch (event.type) {
      case 'model_started':
        setRunState(prev => ({
          ...prev,
          models: prev.models.map(m =>
            m.modelId === event.model_id ? { ...m, status: 'running' } : m,
          ),
        }));
        break;

      case 'model_text_delta': {
        // Buffer the delta; schedule throttled flush
        textBufferRef.current.set(
          event.model_id,
          (textBufferRef.current.get(event.model_id) ?? '') + event.text,
        );
        scheduleFlush();
        break;
      }

      case 'model_complete':
        setRunState(prev => ({
          ...prev,
          models: prev.models.map(m =>
            m.modelId === event.model_id
              ? { ...m, status: 'complete', result: event.result }
              : m,
          ),
        }));
        break;

      case 'model_failed':
        setRunState(prev => ({
          ...prev,
          models: prev.models.map(m =>
            m.modelId === event.model_id
              ? { ...m, status: 'failed', error: event.error }
              : m,
          ),
        }));
        break;

      case 'run_complete':
        setRunState(prev => ({ ...prev, status: 'complete', runId: event.run_id }));
        void loadHistory();
        break;

      case 'run_failed':
        setRunState(prev => ({ ...prev, status: 'failed', error: event.error }));
        break;
    }
  }, [scheduleFlush, loadHistory]);

  // ─── Run start / stop ───────────────────────────────────────────────────────

  const handleStart = useCallback(async () => {
    if (selectedModelIds.length === 0) return;

    // Cancel any previous run
    abortRef.current?.abort();
    const abort = new AbortController();
    abortRef.current = abort;

    // Flush any pending buffer immediately
    if (flushTimerRef.current !== null) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
      textBufferRef.current = new Map();
    }

    // Build initial model states
    const modelStates: ModelResultState[] = selectedModelIds.map(id => {
      const m = models.find(x => x.id === id);
      return {
        modelId: id,
        modelName: m?.name ?? `Model ${id}`,
        status: 'pending',
        liveText: '',
      };
    });

    setRunState({ status: 'running', models: modelStates });

    try {
      if (mode === 'compare') {
        const config: CompareConfig = {
          model_ids: selectedModelIds,
          prompt,
          system_prompt: systemPrompt.trim() || null,
          ctx_size: parseInt(ctxSize, 10) || null,
        };
        await startCompareRun(config, handleEvent, abort.signal);
      } else {
        const config: PerfConfig = {
          model_ids: selectedModelIds,
          pp_tokens: parseInt(ppTokens, 10) || 512,
          tg_tokens: parseInt(tgTokens, 10) || 128,
          repetitions: parseInt(repetitions, 10) || 3,
        };
        await startPerfRun(config, handleEvent, abort.signal);
      }
    } catch (err) {
      if ((err as Error).name !== 'AbortError') {
        setRunState(prev => ({
          ...prev,
          status: 'failed',
          error: (err as Error).message,
        }));
      }
    }
  }, [mode, selectedModelIds, models, prompt, systemPrompt, ctxSize, ppTokens, tgTokens, repetitions, handleEvent]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
    setRunState(prev => ({ ...prev, status: 'idle' }));
  }, []);

  // ─── Model selection helpers ────────────────────────────────────────────────

  const toggleModel = (id: number) => {
    setSelectedModelIds(prev =>
      prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id],
    );
  };

  const isRunning = runState.status === 'running';

  // ─── Render helpers ─────────────────────────────────────────────────────────

  const renderCompareResult = (r: ModelCompareResult) => (
    <div className="flex flex-col gap-sm">
      <div className="bg-surface rounded-md p-base text-sm text-text whitespace-pre-wrap font-mono leading-relaxed">
        {r.response_text}
      </div>
      <div className="flex gap-md text-xs text-text-muted flex-wrap">
        {r.generation_tps != null && <span>⚡ {formatTps(r.generation_tps)} gen</span>}
        {r.prompt_tps != null && <span>{formatTps(r.prompt_tps)} pp</span>}
        {r.generation_ms != null && <span>{formatMs(r.generation_ms)} gen</span>}
        {r.completion_tokens != null && <span>{r.completion_tokens} tokens</span>}
        {r.was_truncated && <span className="text-warning">truncated</span>}
      </div>
    </div>
  );

  const renderPerfResult = (r: ModelPerfResult) => (
    <div className="flex gap-lg flex-wrap text-sm">
      <div className="flex flex-col items-center gap-xs bg-surface rounded-md p-md min-w-[90px]">
        <span className="text-xs text-text-muted">TG Speed</span>
        <span className="text-xl font-bold text-primary">{r.tg_tps.toFixed(1)}</span>
        <span className="text-xs text-text-muted">t/s</span>
      </div>
      <div className="flex flex-col items-center gap-xs bg-surface rounded-md p-md min-w-[90px]">
        <span className="text-xs text-text-muted">PP Speed</span>
        <span className="text-xl font-bold text-success">{r.pp_tps.toFixed(1)}</span>
        <span className="text-xs text-text-muted">t/s</span>
      </div>
      <div className="flex flex-col items-center gap-xs bg-surface rounded-md p-md min-w-[90px]">
        <span className="text-xs text-text-muted">Backend</span>
        <span className="text-sm font-medium text-text">{r.backend ?? '—'}</span>
      </div>
      <div className="flex flex-col items-center gap-xs bg-surface rounded-md p-md min-w-[90px]">
        <span className="text-xs text-text-muted">Reps</span>
        <span className="text-sm font-medium text-text">{r.repetitions}</span>
      </div>
    </div>
  );

  const renderModelCard = (m: ModelResultState) => {
    const statusColor = {
      pending: 'text-text-muted',
      running: 'text-primary',
      complete: 'text-success',
      failed: 'text-danger',
    }[m.status];

    const statusLabel = {
      pending: 'Pending',
      running: 'Running…',
      complete: 'Complete',
      failed: 'Failed',
    }[m.status];

    return (
      <div key={m.modelId} className="border border-border rounded-lg p-base flex flex-col gap-sm">
        <div className="flex items-center gap-sm">
          <span className="font-medium text-text truncate flex-1">{m.modelName}</span>
          <span className={cn('text-xs font-medium', statusColor)}>{statusLabel}</span>
        </div>

        {/* Live streaming text (compare mode) */}
        {m.status === 'running' && m.liveText && mode === 'compare' && (
          <div className="bg-surface rounded-md p-base text-sm text-text whitespace-pre-wrap font-mono leading-relaxed max-h-[300px] overflow-y-auto">
            {m.liveText}
            <span className="inline-block w-2 h-4 bg-primary animate-pulse ml-0.5 align-text-bottom" />
          </div>
        )}

        {/* Completed result */}
        {m.status === 'complete' && m.result && (
          <>
            {m.result.kind === 'compare' && renderCompareResult(m.result)}
            {m.result.kind === 'perf' && renderPerfResult(m.result)}
          </>
        )}

        {/* Error */}
        {m.status === 'failed' && m.error && (
          <div className="text-sm text-danger bg-danger-subtle rounded-md p-sm">{m.error}</div>
        )}
      </div>
    );
  };

  // ─── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full w-full bg-background overflow-hidden">
      {/* Header */}
      <header className="flex items-center gap-md px-base py-sm border-b border-border shrink-0">
        <Button variant="ghost" size="sm" iconOnly onClick={onClose} aria-label="Close benchmark">
          <Icon icon={ArrowLeft} size={16} />
        </Button>
        <Icon icon={BarChart2} size={18} className="text-primary" />
        <h1 className="text-base font-semibold text-text m-0 flex-1">Benchmark Dashboard</h1>
        <div className="flex gap-xs">
          <Button
            variant={mode === 'perf' ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => setMode('perf')}
          >
            Perf
          </Button>
          <Button
            variant={mode === 'compare' ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => setMode('compare')}
          >
            Compare
          </Button>
          <Button
            variant={mode === 'tune' ? 'primary' : 'secondary'}
            size="sm"
            leftIcon={<Icon icon={Target} size={14} />}
            onClick={() => setMode('tune')}
          >
            Tune
          </Button>
        </div>
      </header>

      {/* Body */}
      {mode === 'tune' ? (
        <TuneTab models={models} />
      ) : (
      <div className="flex flex-1 overflow-hidden gap-0">
        {/* ── Left: Config panel ── */}
        <aside className="w-[280px] shrink-0 flex flex-col gap-base p-base border-r border-border overflow-y-auto">
          {/* Model selection */}
          <div className="flex flex-col gap-sm">
            <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
              Models ({selectedModelIds.length} selected)
            </label>
            <div className="flex flex-col gap-xs max-h-[240px] overflow-y-auto border border-border rounded-md p-xs">
              {models.length === 0 && (
                <p className="text-xs text-text-muted p-xs">No models available</p>
              )}
              {models.map(m => {
                if (m.id == null) return null;
                const checked = selectedModelIds.includes(m.id);
                return (
                  <label
                    key={m.id}
                    className={cn(
                      'flex items-center gap-sm p-xs rounded-sm cursor-pointer text-sm transition-colors',
                      checked ? 'bg-primary-subtle text-text' : 'text-text-secondary hover:bg-surface-elevated',
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleModel(m.id!)}
                      className="accent-primary"
                    />
                    <span className="truncate">{m.name}</span>
                  </label>
                );
              })}
            </div>
          </div>

          {/* Compare config */}
          {mode === 'compare' && (
            <>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  Prompt
                </label>
                <Textarea
                  value={prompt}
                  onChange={e => setPrompt(e.target.value)}
                  rows={4}
                  disabled={isRunning}
                  placeholder="Enter prompt…"
                />
              </div>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  System Prompt (optional)
                </label>
                <Textarea
                  value={systemPrompt}
                  onChange={e => setSystemPrompt(e.target.value)}
                  rows={2}
                  disabled={isRunning}
                  placeholder="Optional system prompt…"
                />
              </div>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  Context Size (optional)
                </label>
                <Input
                  type="number"
                  value={ctxSize}
                  onChange={e => setCtxSize(e.target.value)}
                  disabled={isRunning}
                  min={512}
                  size="sm"
                  placeholder="Default from settings"
                />
              </div>
            </>
          )}

          {/* Perf config */}
          {mode === 'perf' && (
            <>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  PP Tokens
                </label>
                <Input
                  type="number"
                  value={ppTokens}
                  onChange={e => setPpTokens(e.target.value)}
                  disabled={isRunning}
                  min={1}
                  size="sm"
                />
              </div>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  TG Tokens
                </label>
                <Input
                  type="number"
                  value={tgTokens}
                  onChange={e => setTgTokens(e.target.value)}
                  disabled={isRunning}
                  min={1}
                  size="sm"
                />
              </div>
              <div className="flex flex-col gap-xs">
                <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
                  Repetitions
                </label>
                <Input
                  type="number"
                  value={repetitions}
                  onChange={e => setRepetitions(e.target.value)}
                  disabled={isRunning}
                  min={1}
                  max={10}
                  size="sm"
                />
              </div>
            </>
          )}

          {/* Run / Stop button */}
          <Button
            variant={isRunning ? 'danger' : 'primary'}
            size="lg"
            fullWidth
            disabled={selectedModelIds.length === 0}
            onClick={isRunning ? handleStop : handleStart}
            leftIcon={<Icon icon={isRunning ? Square : Play} size={16} />}
          >
            {isRunning ? 'Stop' : 'Run'}
          </Button>

          {runState.status === 'failed' && runState.error && (
            <div className="text-xs text-danger bg-danger-subtle rounded-md p-sm">
              {runState.error}
            </div>
          )}
        </aside>

        {/* ── Right: Results + History ── */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Live results area */}
          <div className="flex-1 overflow-y-auto p-base flex flex-col gap-base">
            {runState.models.length === 0 && runState.status === 'idle' && (
              <div className="flex flex-col items-center justify-center h-full text-text-muted gap-sm">
                <Icon icon={Zap} size={32} className="opacity-30" />
                <p className="text-sm">Select models and press Run to start a benchmark.</p>
              </div>
            )}
            {runState.models.map(renderModelCard)}
          </div>

          {/* History section */}
          <section className="border-t border-border shrink-0">
            <div className="flex items-center gap-sm px-base py-sm border-b border-border">
              <h2 className="text-sm font-semibold text-text m-0 flex-1">Recent Runs</h2>
              <Button
                variant="ghost"
                size="sm"
                onClick={loadHistory}
                disabled={historyLoading}
              >
                {historyLoading ? 'Loading…' : 'Refresh'}
              </Button>
            </div>
            <div className="overflow-x-auto max-h-[220px] overflow-y-auto">
              {history.length === 0 ? (
                <p className="text-xs text-text-muted p-base">No benchmark runs yet.</p>
              ) : (
                <table className="w-full text-xs border-collapse">
                  <thead className="sticky top-0 bg-background z-10">
                    <tr className="text-left text-text-muted border-b border-border">
                      <th className="px-base py-xs font-medium">ID</th>
                      <th className="px-base py-xs font-medium">Type</th>
                      <th className="px-base py-xs font-medium">Status</th>
                      <th className="px-base py-xs font-medium">Models</th>
                      <th className="px-base py-xs font-medium">Started</th>
                    </tr>
                  </thead>
                  <tbody>
                    {history.map(run => (
                      <tr key={run.id} className="border-b border-border-light hover:bg-surface-elevated transition-colors">
                        <td className="px-base py-xs text-text-secondary">{run.id}</td>
                        <td className="px-base py-xs">
                          <span className={cn(
                            'py-xs px-sm rounded-sm font-medium',
                            run.run_type === 'perf' ? 'bg-primary-subtle text-primary' : 'bg-warning-subtle text-warning',
                          )}>
                            {run.run_type}
                          </span>
                        </td>
                        <td className="px-base py-xs">
                          <span className={cn(
                            'py-xs px-sm rounded-sm font-medium',
                            run.status === 'complete' && 'text-success bg-success-subtle',
                            run.status === 'running' && 'text-primary bg-primary-subtle',
                            run.status === 'failed' && 'text-danger bg-danger-subtle',
                          )}>
                            {run.status}
                          </span>
                        </td>
                        <td className="px-base py-xs text-text-secondary">{run.model_ids.length}</td>
                        <td className="px-base py-xs text-text-muted">{formatDate(run.created_at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          </section>
        </div>
      </div>
      )}
    </div>
  );
};

export default BenchmarkPage;
