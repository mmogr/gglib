import { FC, useState } from 'react';
import { Play, Square } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { Select } from '../../ui/Select';
import { Checkbox } from '../../ui/Checkbox';
import { SuitePicker } from '../SuitePicker';
import type { GgufModel } from '../../../types';
import type { AgenticEvalConfig, TaskSuite } from '../../../types/benchmark';

/** Mirrors the server's DEFAULT_SEEDS so the form shows what an untouched run uses. */
const DEFAULT_SEEDS_TEXT = '12345, 67890, 11111';

interface AgenticConfigFormProps {
  models: GgufModel[];
  isRunning: boolean;
  onSubmit: (config: AgenticEvalConfig) => void;
  onStop: () => void;
}

/**
 * Agentic A/B eval configuration: model, seeds, arms, and task suite.
 * Owns form values only — emits a fully-built `AgenticEvalConfig`.
 */
export const AgenticConfigForm: FC<AgenticConfigFormProps> = ({
  models,
  isRunning,
  onSubmit,
  onStop,
}) => {
  const [modelId, setModelId] = useState<number | ''>(
    models.length > 0 && models[0].id != null ? models[0].id : '',
  );
  const [seedsText, setSeedsText] = useState(DEFAULT_SEEDS_TEXT);
  const [ctxSize, setCtxSize] = useState('');
  const [includeControl, setIncludeControl] = useState(true);
  const [replicateRaw, setReplicateRaw] = useState(true);
  const [controlSeeds, setControlSeeds] = useState('1');
  const [suite, setSuite] = useState<TaskSuite | null>({ source: 'default' });

  // Only whole decimal tokens count, capped at u32::MAX — anything else
  // would fail serde on the Rust side with an opaque 422.
  const seeds = seedsText
    .split(',')
    .map((s) => s.trim())
    .filter((t) => /^\d+$/.test(t))
    .map((t) => parseInt(t, 10))
    .filter((n) => n <= 4294967295);

  const handleSubmit = () => {
    if (modelId === '' || suite == null) return;
    const ctx = parseInt(ctxSize, 10);
    onSubmit({
      model_id: modelId,
      task_suite: suite,
      ctx_size: Number.isFinite(ctx) && ctx >= 512 ? ctx : null,
      seeds,
      include_control: includeControl,
      replicate_raw: replicateRaw,
      control_seeds: Math.max(1, parseInt(controlSeeds, 10) || 1),
    });
  };

  return (
    <div className="flex flex-col gap-base">
      <div className="flex flex-col gap-xs">
        <label htmlFor="agentic-model" className="text-xs font-semibold text-text">Model</label>
        <Select
          id="agentic-model"
          size="sm"
          value={modelId}
          disabled={isRunning}
          onChange={(e) => setModelId(e.target.value ? Number(e.target.value) : '')}
        >
          <option value="">Select a model…</option>
          {models.map((m) => (
            <option key={m.id} value={m.id ?? ''}>
              {m.name}
            </option>
          ))}
        </Select>
      </div>

      <div className="flex flex-col gap-xs">
        <label htmlFor="agentic-seeds" className="text-xs font-semibold text-text">Seeds</label>
        <Input
          id="agentic-seeds"
          type="text"
          size="sm"
          className="font-mono tabular-nums"
          value={seedsText}
          disabled={isRunning}
          onChange={(e) => setSeedsText(e.target.value)}
          placeholder="Comma-separated, empty = one unseeded run"
        />
        <p className="text-xs text-text-muted m-0">
          {seeds.length >= 2
            ? `Raw and gglib arms run every task once per seed (${seeds.length}×); the control repeats ${Math.min(seeds.length, Math.max(1, parseInt(controlSeeds, 10) || 1))} of them.`
            : 'A single sample carries full decode variance — two or more seeds make the report a measurement.'}
        </p>
      </div>

      <div className="flex flex-col gap-xs">
        <label htmlFor="agentic-ctx" className="text-xs font-semibold text-text">Context Size (optional)</label>
        <Input
          id="agentic-ctx"
          type="number"
          size="sm"
          className="font-mono tabular-nums"
          value={ctxSize}
          disabled={isRunning}
          min={512}
          onChange={(e) => setCtxSize(e.target.value)}
          placeholder="512 or more; blank = default from settings"
        />
      </div>

      <div className="flex flex-col gap-sm">
        <label className="text-xs font-semibold text-text">Validity arms</label>
        <p className="text-xs text-text-muted m-0">
          Every task runs through the raw pipeline (gglib bypassed) and the full gglib pipeline;
          the arms below keep that comparison honest.
        </p>
        <Checkbox
          checked={includeControl}
          disabled={isRunning}
          onChange={(e) => setIncludeControl(e.target.checked)}
          label="Positive control (sampling deliberately broken)"
        />
        {includeControl && (
          <div className="flex items-center gap-sm pl-lg">
            <label htmlFor="agentic-control-seeds" className="text-xs text-text-muted">
              Control seeds
            </label>
            <Input
              id="agentic-control-seeds"
              type="number"
              size="sm"
              className="w-16 font-mono tabular-nums"
              value={controlSeeds}
              disabled={isRunning}
              min={1}
              onChange={(e) => setControlSeeds(e.target.value)}
            />
          </div>
        )}
        <Checkbox
          checked={replicateRaw}
          disabled={isRunning}
          onChange={(e) => setReplicateRaw(e.target.checked)}
          label="A/A replicate (raw again, disjoint seeds)"
        />
      </div>

      <SuitePicker disabled={isRunning} onSuiteChange={setSuite} />

      <Button
        variant={isRunning ? 'dangerGhost' : 'primary'}
        size="lg"
        fullWidth
        disabled={!isRunning && (modelId === '' || suite == null)}
        onClick={isRunning ? onStop : handleSubmit}
        leftIcon={<Icon icon={isRunning ? Square : Play} size={16} />}
      >
        {isRunning ? 'Stop' : 'Run Eval'}
      </Button>
    </div>
  );
};
