/**
 * Settings › Models › Sampling defaults: one line saying what is set, and
 * Change… opening the sampling form in place, the fields set and "Add a
 * value" for the rest. Save writes `inferenceDefaults` alone, and nothing
 * when the form ends as it began; Undo puts the defaults before back.
 *
 * @module places/settings/SamplingDefaults
 */

import { useState, type FC } from 'react';
import { SamplingForm, type CompactKey } from '../../components/SamplingForm';
import { Button } from '../../components/ui/Button';
import { REASONING_KEYS, SAMPLING_KEYS } from '../../inspector/local/rows';
import type { SparseInferenceConfig } from '../../types';
import { useStored } from './fields';
import { summarize } from './InferenceProfiles';
import { SaveLine } from './SaveLine';
import { useSettingSave } from './useFieldSave';

const KEYS = [...SAMPLING_KEYS, ...REASONING_KEYS] as readonly CompactKey[];

export const SamplingDefaults: FC = () => {
  const stored = useStored('inferenceDefaults') as SparseInferenceConfig | null;
  const save = useSettingSave('inferenceDefaults', stored);
  const [draft, setDraft] = useState<SparseInferenceConfig | null>(null);
  const set = stored !== null && Object.values(stored).some((v) => v !== null && v !== undefined);

  const submit = async () => {
    if (draft !== null && (await save.commit(draft))) setDraft(null);
  };

  return (
    <section aria-label="Sampling defaults" className="flex flex-col gap-sm">
      <div className="flex items-baseline gap-sm">
        <span className="text-sm font-semibold text-text">Sampling defaults</span>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-text-secondary">
          {set ? summarize(stored) : 'None set'}
        </span>
        {draft === null && (
          <Button variant="link" size="sm" onClick={() => setDraft(stored ?? {})}>
            Change…
          </Button>
        )}
      </div>
      {draft !== null && (
        <>
          <SamplingForm
            value={draft}
            onChange={setDraft}
            fallback={{ kind: 'upstream' }}
            compact={{
              keys: KEYS,
              caption: "For every model. A model's own values outrank these; a field left empty uses the model's recipe, and beneath it llama.cpp's own default.",
            }}
          />
          <span className="flex gap-xs">
            <Button size="sm" variant="secondary" isLoading={save.saving} onClick={() => void submit()}>
              Save
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setDraft(null)}>
              Cancel
            </Button>
          </span>
        </>
      )}
      <SaveLine save={save} />
    </section>
  );
};
