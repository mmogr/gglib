/**
 * A row's control, drawn in its place by Change: Context a number field,
 * Templates and Speculative a select, Images and Parts the file menus,
 * Sampling and Reasoning the sampling form showing only the fields set. A
 * save sends the one field of the update it changes (`serverDefaults` or
 * `inferenceDefaults`, the other values in it kept as stored), then the
 * owner reads the explanation again. `reset` is what each row's Reset sends.
 *
 * @module inspector/local/Change
 */

import { useState, type FC } from 'react';
import { SamplingForm, type CompactKey } from '../../components/SamplingForm';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import type { ModelUpdates } from '../../services/modelRegistry';
import type { GgufModel, ModelDetail, SamplingExplanation, ServerConfig, SparseInferenceConfig } from '../../types';
import { formatError } from '../../utils/errors';
import { ComponentPickers } from './ComponentPickers';
import { ProjectorPicker } from './ProjectorPicker';
import { REASONING_KEYS, SAMPLING_KEYS } from './rows';

export type Update = (id: number, updates: ModelUpdates) => Promise<void>;

/** The rows Change can turn into a control, by decision key. */
export const CHANGEABLE: ReadonlySet<string> = new Set(['ctx', 'images', 'parts', 'sampling', 'reasoning', 'templates', 'mtp']);

const stored = (model: GgufModel): ServerConfig => model.serverDefaults ?? { contextLength: null };

/** `inferenceDefaults` with `keys` cleared. */
function without(model: GgufModel, keys: readonly string[]): SparseInferenceConfig {
  const next: Record<string, unknown> = { ...(model.inferenceDefaults ?? {}) };
  for (const key of keys) delete next[key];
  return next as SparseInferenceConfig;
}

/** What a row's Reset sends: its stored value cleared, the rest kept. */
export function reset(key: string, model: GgufModel): ModelUpdates | null {
  if (key === 'ctx') return { serverDefaults: { ...stored(model), contextLength: null } };
  if (key === 'templates') return { serverDefaults: { ...stored(model), jinja: null } };
  if (key === 'mtp') return { serverDefaults: { ...stored(model), mtp: null } };
  if (key === 'sampling') return { inferenceDefaults: without(model, SAMPLING_KEYS) };
  if (key === 'reasoning') return { inferenceDefaults: without(model, REASONING_KEYS) };
  return null;
}

interface ChangeProps {
  kind: string;
  model: GgufModel;
  detail: ModelDetail | null;
  sampling: SamplingExplanation | null;
  /** The context it would be served at, the field's placeholder. */
  servesAt: number | null;
  /** Said once over the control: "applies at next load" on a loaded model. */
  note?: string;
  onUpdateModel: Update;
  /** The change was saved (or the row closed): read the explanation again. */
  onDone: (saved: boolean) => void;
}

const TRISTATE = [
  ['', 'Auto'],
  ['on', 'On'],
  ['off', 'Off'],
] as const;

const tristate = (value: boolean | null | undefined) => (value == null ? '' : value ? 'on' : 'off');

export const Change: FC<ChangeProps> = ({ kind, model, detail, sampling, servesAt, note, onUpdateModel, onDone }) => {
  const [context, setContext] = useState(model.serverDefaults?.contextLength?.toString() ?? '');
  const [draft, setDraft] = useState<SparseInferenceConfig | undefined>(model.inferenceDefaults ?? undefined);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async (updates: ModelUpdates) => {
    setSaving(true);
    setError(null);
    try {
      await onUpdateModel(model.id, updates);
      onDone(true);
    } catch (err) {
      setError(formatError(err));
    } finally {
      setSaving(false);
    }
  };

  const buttons = (onSave?: () => void) => (
    <span className="flex gap-xs">
      {onSave && (
        <Button size="sm" variant="secondary" onClick={onSave} isLoading={saving}>
          Save
        </Button>
      )}
      <Button size="sm" variant="ghost" onClick={() => onDone(!onSave)}>
        {onSave ? 'Cancel' : 'Done'}
      </Button>
    </span>
  );

  let control;
  if (kind === 'ctx') {
    const n = Number(context);
    const valid = context.trim() !== '' && Number.isInteger(n) && n > 0;
    const submit = () => valid && void save({ serverDefaults: { ...stored(model), contextLength: n } });
    control = (
      <>
        <Input
          size="sm"
          type="number"
          min={1}
          aria-label="Context"
          value={context}
          placeholder={servesAt?.toString()}
          onChange={(e) => setContext(e.target.value)}
          onKeyDown={(e) => (e.key === 'Enter' ? submit() : e.key === 'Escape' && onDone(false))}
          className="max-w-[10rem]"
        />
        {buttons(valid ? submit : undefined)}
      </>
    );
  } else if (kind === 'templates' || kind === 'mtp') {
    const field = kind === 'mtp' ? 'mtp' : 'jinja';
    control = (
      <>
        <Select
          size="sm"
          aria-label={kind === 'mtp' ? 'Speculative' : 'Templates'}
          value={tristate(model.serverDefaults?.[field])}
          disabled={saving}
          onChange={(e) =>
            void save({ serverDefaults: { ...stored(model), [field]: e.target.value === '' ? null : e.target.value === 'on' } })
          }
          className="max-w-[10rem]"
        >
          {TRISTATE.map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </Select>
        {buttons()}
      </>
    );
  } else if (kind === 'images' || kind === 'parts') {
    const Picker = kind === 'images' ? ProjectorPicker : ComponentPickers;
    control = (
      <>
        <Picker modelId={model.id} detail={detail ?? undefined} onUpdateModel={onUpdateModel} onChanged={() => onDone(true)} />
        {buttons()}
      </>
    );
  } else {
    const keys = (kind === 'reasoning' ? REASONING_KEYS : SAMPLING_KEYS) as readonly CompactKey[];
    control = (
      <>
        <SamplingForm
          value={draft}
          onChange={setDraft}
          fallback={{
            kind: 'resolved',
            ownLayer: 'modelUserSet',
            resolution: { explanation: sampling, isLoading: false, hasError: sampling === null },
          }}
          capabilities={detail ? { reasoningEffort: detail.reasoningEffortSupport } : undefined}
          compact={{ keys, caption: "This model's own values. A field left empty uses the setting below it." }}
        />
        {buttons(() => void save({ inferenceDefaults: draft ?? {} }))}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-sm">
      {note && <span className="text-text-secondary">{note}</span>}
      {control}
      {error && <span className="text-danger">{error}</span>}
    </div>
  );
};
