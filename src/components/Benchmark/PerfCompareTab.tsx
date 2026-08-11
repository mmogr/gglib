/**
 * Perf and Compare benchmark modes: config aside + live results column.
 * Run state and streaming live in `usePerfCompareRun`.
 *
 * @module components/Benchmark/PerfCompareTab
 */

import { FC, useState } from 'react';
import { cn } from '../../utils/cn';
import { Play, Square, Zap } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Input } from '../ui/Input';
import { Textarea } from '../ui/Textarea';
import { EmptyState } from '../primitives';
import { ModelMultiSelect } from './ModelMultiSelect';
import { PerfCompareResultCard } from './PerfCompareResultCard';
import { usePerfCompareRun } from './usePerfCompareRun';
import type { GgufModel } from '../../types';

interface PerfCompareTabProps {
  mode: 'perf' | 'compare';
  /** False while another benchmark tab is shown — the tab stays mounted so runs and form state survive. */
  active?: boolean;
  models: GgufModel[];
  initialModelIds?: number[];
  /** Called when a run finishes, so the page can refresh the shared history. */
  onRunComplete: () => void;
}

export const PerfCompareTab: FC<PerfCompareTabProps> = ({
  mode,
  active = true,
  models,
  initialModelIds,
  onRunComplete,
}) => {
  const [selectedModelIds, setSelectedModelIds] = useState<number[]>(
    initialModelIds ?? (models.length > 0 && models[0].id != null ? [models[0].id] : []),
  );
  const [prompt, setPrompt] = useState('Tell me a short story about a robot.');
  const [systemPrompt, setSystemPrompt] = useState('');
  const [ctxSize, setCtxSize] = useState('');
  const [ppTokens, setPpTokens] = useState('512');
  const [tgTokens, setTgTokens] = useState('128');
  const [repetitions, setRepetitions] = useState('3');

  const { runState, start, stop } = usePerfCompareRun(models, onRunComplete);
  const isRunning = runState.status === 'running';

  const handleStart = () => {
    if (selectedModelIds.length === 0) return;
    if (mode === 'compare') {
      void start(
        {
          model_ids: selectedModelIds,
          prompt,
          system_prompt: systemPrompt.trim() || null,
          ctx_size: parseInt(ctxSize, 10) || null,
        },
        'compare',
        selectedModelIds,
      );
    } else {
      void start(
        {
          model_ids: selectedModelIds,
          pp_tokens: parseInt(ppTokens, 10) || 512,
          tg_tokens: parseInt(tgTokens, 10) || 128,
          repetitions: parseInt(repetitions, 10) || 3,
        },
        'perf',
        selectedModelIds,
      );
    }
  };

  const toggleModel = (id: number) => {
    setSelectedModelIds((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );
  };

  const numberField = (
    label: string,
    value: string,
    onChange: (v: string) => void,
    extra?: { min?: number; max?: number; placeholder?: string },
  ) => (
    <div className="flex flex-col gap-xs">
      <label className="text-xs font-semibold text-text">{label}</label>
      <Input
        type="number"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={isRunning}
        size="sm"
        className="font-mono tabular-nums"
        {...extra}
      />
    </div>
  );

  return (
    <div className={cn('flex flex-1 overflow-hidden gap-0', !active && 'hidden')}>
      <aside className="w-[280px] shrink-0 flex flex-col gap-base p-base border-r border-border overflow-y-auto">
        <ModelMultiSelect
          models={models}
          selectedIds={selectedModelIds}
          onToggle={toggleModel}
        />

        {mode === 'compare' && (
          <>
            <div className="flex flex-col gap-xs">
              <label className="text-xs font-semibold text-text">Prompt</label>
              <Textarea
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                rows={4}
                disabled={isRunning}
                placeholder="Enter prompt…"
              />
            </div>
            <div className="flex flex-col gap-xs">
              <label className="text-xs font-semibold text-text">System Prompt (optional)</label>
              <Textarea
                value={systemPrompt}
                onChange={(e) => setSystemPrompt(e.target.value)}
                rows={2}
                disabled={isRunning}
                placeholder="Optional system prompt…"
              />
            </div>
            {numberField('Context Size (optional)', ctxSize, setCtxSize, {
              min: 512,
              placeholder: 'Default from settings',
            })}
          </>
        )}

        {mode === 'perf' && (
          <>
            {numberField('PP Tokens', ppTokens, setPpTokens, { min: 1 })}
            {numberField('TG Tokens', tgTokens, setTgTokens, { min: 1 })}
            {numberField('Repetitions', repetitions, setRepetitions, { min: 1, max: 10 })}
          </>
        )}

        <Button
          variant={isRunning ? 'dangerGhost' : 'primary'}
          size="lg"
          fullWidth
          disabled={selectedModelIds.length === 0}
          onClick={isRunning ? stop : handleStart}
          leftIcon={<Icon icon={isRunning ? Square : Play} size={16} />}
        >
          {isRunning ? 'Stop' : 'Run'}
        </Button>

        {runState.status === 'failed' && runState.error && (
          <div className="text-xs text-danger bg-danger-subtle rounded-md p-sm">{runState.error}</div>
        )}
      </aside>

      <div className="flex-1 overflow-y-auto p-base flex flex-col gap-base">
        {runState.models.length === 0 && runState.status === 'idle' && (
          <EmptyState
            className="h-full"
            icon={<Icon icon={Zap} size={24} />}
            title="No benchmark yet"
            description="Pick one or more models on the left, then press Run to measure prompt-processing and generation speed."
          />
        )}
        {runState.models.map((m) => (
          <PerfCompareResultCard key={m.modelId} model={m} showLiveText={mode === 'compare'} />
        ))}
      </div>
    </div>
  );
};
