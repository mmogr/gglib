/**
 * Tune-mode configuration form: model select, sampling-parameter sweep
 * inputs, task-suite selector (built-in default vs. custom file upload),
 * seeding toggles, pruning/weight overrides, and Apply-best checkbox.
 *
 * Emits a fully-built `TuneConfig` via `onSubmit` — this component owns no
 * SSE/run state, only the form values.
 *
 * @module components/Benchmark/Tune/TuneConfigForm
 */

import { FC, useState } from 'react';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';
import type { GgufModel } from '../../../types';
import type { TuneConfig, TuneTask } from '../../../types/benchmark';

interface TuneConfigFormProps {
  models: GgufModel[];
  disabled: boolean;
  onSubmit: (config: TuneConfig, applyBest: boolean) => void;
}

/** Parse a comma-separated numeric list, ignoring blank/invalid entries. */
function parseNumberList(value: string): number[] {
  return value
    .split(',')
    .map(v => v.trim())
    .filter(v => v.length > 0)
    .map(Number)
    .filter(n => !Number.isNaN(n));
}

export const TuneConfigForm: FC<TuneConfigFormProps> = ({ models, disabled, onSubmit }) => {
  const [modelId, setModelId] = useState<number | ''>(
    models.length > 0 && models[0].id != null ? models[0].id : '',
  );
  const [temperature, setTemperature] = useState('0.2,0.5,0.8');
  const [topP, setTopP] = useState('');
  const [topK, setTopK] = useState('');
  const [minP, setMinP] = useState('');
  const [repeatPenalty, setRepeatPenalty] = useState('');

  const [suiteMode, setSuiteMode] = useState<'default' | 'custom'>('default');
  const [customTasks, setCustomTasks] = useState<TuneTask[] | null>(null);
  const [customSuiteError, setCustomSuiteError] = useState<string | null>(null);

  const [seedFromGguf, setSeedFromGguf] = useState(true);
  const [seedFromFamilyPresets, setSeedFromFamilyPresets] = useState(true);
  const [pruneFraction, setPruneFraction] = useState('0.5');
  const [ctxSize, setCtxSize] = useState('');
  const [applyBest, setApplyBest] = useState(false);

  // Client-side file parsing: the uploaded file must be a plain JSON array
  // of TuneTask values — the identical shape `--task-suite path.json` reads
  // from disk on the CLI. Parsed here, not sent as a raw file upload.
  const handleFileChange = (file: File | null) => {
    setCustomSuiteError(null);
    setCustomTasks(null);
    if (!file) return;
    file
      .text()
      .then(text => {
        const parsed = JSON.parse(text) as unknown;
        if (!Array.isArray(parsed)) {
          throw new Error('expected a JSON array of task definitions');
        }
        setCustomTasks(parsed as TuneTask[]);
      })
      .catch(err => {
        setCustomSuiteError(
          `Failed to parse task suite file: ${(err as Error).message}`,
        );
      });
  };

  const canSubmit =
    modelId !== '' && (suiteMode === 'default' || (customTasks?.length ?? 0) > 0);

  const handleSubmit = () => {
    if (modelId === '') return;

    const config: TuneConfig = {
      model_id: modelId,
      task_suite:
        suiteMode === 'default'
          ? { source: 'default' }
          : { source: 'custom', tasks: customTasks ?? [] },
      sweep: {
        temperature: parseNumberList(temperature),
        top_p: parseNumberList(topP),
        top_k: parseNumberList(topK),
        min_p: parseNumberList(minP),
        repeat_penalty: parseNumberList(repeatPenalty),
      },
      seed_from_gguf: seedFromGguf,
      seed_from_family_presets: seedFromFamilyPresets,
      weights: {
        tool_accuracy: 0.4,
        loop_avoidance: 0.3,
        task_completion: 0.2,
        speed: 0.1,
      },
      prune_fraction: parseFloat(pruneFraction) || 0.5,
      ctx_size: ctxSize.trim() ? parseInt(ctxSize, 10) : null,
    };
    onSubmit(config, applyBest);
  };

  return (
    <div className="flex flex-col gap-base">
      <div className="flex flex-col gap-xs">
        <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
          Model
        </label>
        <select
          className="w-full bg-surface border border-border rounded-md px-sm py-xs text-sm text-text"
          value={modelId}
          disabled={disabled}
          onChange={e => setModelId(e.target.value ? Number(e.target.value) : '')}
        >
          <option value="">Select a model…</option>
          {models.map(m => (
            <option key={m.id} value={m.id ?? ''}>
              {m.name}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-col gap-xs">
        <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
          Sweep — temperature
        </label>
        <Input
          value={temperature}
          onChange={e => setTemperature(e.target.value)}
          disabled={disabled}
          size="sm"
          placeholder="0.2,0.5,0.8"
        />
      </div>
      <div className="grid grid-cols-2 gap-sm">
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            top_p
          </label>
          <Input value={topP} onChange={e => setTopP(e.target.value)} disabled={disabled} size="sm" placeholder="0.9,0.95" />
        </div>
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            top_k
          </label>
          <Input value={topK} onChange={e => setTopK(e.target.value)} disabled={disabled} size="sm" placeholder="20,40" />
        </div>
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            min_p
          </label>
          <Input value={minP} onChange={e => setMinP(e.target.value)} disabled={disabled} size="sm" placeholder="0,0.05" />
        </div>
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            repeat_penalty
          </label>
          <Input value={repeatPenalty} onChange={e => setRepeatPenalty(e.target.value)} disabled={disabled} size="sm" placeholder="1.0,1.1" />
        </div>
      </div>

      <div className="flex flex-col gap-xs">
        <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
          Task Suite
        </label>
        <div className="flex gap-sm">
          <Button
            variant={suiteMode === 'default' ? 'primary' : 'secondary'}
            size="sm"
            fullWidth
            disabled={disabled}
            onClick={() => setSuiteMode('default')}
          >
            Default
          </Button>
          <Button
            variant={suiteMode === 'custom' ? 'primary' : 'secondary'}
            size="sm"
            fullWidth
            disabled={disabled}
            onClick={() => setSuiteMode('custom')}
          >
            Custom
          </Button>
        </div>
        {suiteMode === 'custom' && (
          <div className="flex flex-col gap-xs">
            <input
              type="file"
              accept=".json,application/json"
              disabled={disabled}
              onChange={e => handleFileChange(e.target.files?.[0] ?? null)}
              className="text-xs text-text-secondary"
            />
            {customTasks && (
              <p className="text-xs text-success">{customTasks.length} task(s) loaded</p>
            )}
            {customSuiteError && <p className="text-xs text-danger">{customSuiteError}</p>}
          </div>
        )}
      </div>

      <div className="flex flex-col gap-xs">
        <label className="flex items-center gap-sm text-sm text-text-secondary">
          <input
            type="checkbox"
            checked={seedFromGguf}
            disabled={disabled}
            onChange={e => setSeedFromGguf(e.target.checked)}
            className="accent-primary"
          />
          Seed from GGUF author defaults
        </label>
        <label className="flex items-center gap-sm text-sm text-text-secondary">
          <input
            type="checkbox"
            checked={seedFromFamilyPresets}
            disabled={disabled}
            onChange={e => setSeedFromFamilyPresets(e.target.checked)}
            className="accent-primary"
          />
          Seed from family presets
        </label>
      </div>

      <div className="grid grid-cols-2 gap-sm">
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            Prune fraction
          </label>
          <Input value={pruneFraction} onChange={e => setPruneFraction(e.target.value)} disabled={disabled} size="sm" />
        </div>
        <div className="flex flex-col gap-xs">
          <label className="text-xs font-semibold text-text-secondary uppercase tracking-wide">
            Context size
          </label>
          <Input value={ctxSize} onChange={e => setCtxSize(e.target.value)} disabled={disabled} size="sm" placeholder="Default" />
        </div>
      </div>

      <label className="flex items-center gap-sm text-sm text-text-secondary">
        <input
          type="checkbox"
          checked={applyBest}
          disabled={disabled}
          onChange={e => setApplyBest(e.target.checked)}
          className="accent-primary"
        />
        Apply best config to model when complete
      </label>

      <Button
        variant="primary"
        size="lg"
        fullWidth
        disabled={disabled || !canSubmit}
        onClick={handleSubmit}
      >
        Run Tune
      </Button>
    </div>
  );
};

export default TuneConfigForm;
