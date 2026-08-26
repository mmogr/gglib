import { FC } from 'react';
import { Select } from '../../ui/Select';
import type { GgufModel } from '../../../types';
import { CONTEXT_SIZE } from '../../../constants/settingsDefaults';
import { NumberSettingField } from './NumberSettingField';
import { SettingField } from './SettingField';

interface ModelDefaultsProps {
  contextSizeInput: string;
  setContextSizeInput: (value: string) => void;
  defaultModelInput: string;
  setDefaultModelInput: (value: string) => void;
  models: GgufModel[];
  loadingModels: boolean;
  saving: boolean;
}

/**
 * Default context size and default model selection.
 */
export const ModelDefaults: FC<ModelDefaultsProps> = ({
  contextSizeInput,
  setContextSizeInput,
  defaultModelInput,
  setDefaultModelInput,
  models,
  loadingModels,
  saving,
}) => (
  <>
    <NumberSettingField
      id="context-size-input"
      label="Default Context Size"
      spec={CONTEXT_SIZE}
      value={contextSizeInput}
      onChange={setContextSizeInput}
      description="Context window for models with no per-model override."
      disabled={saving}
    />

    <SettingField
      id="default-model-select"
      label="Default Model"
      description={
        <>
          Model to use for quick commands like <code>gglib question</code>
        </>
      }
    >
      <Select
        id="default-model-select"
        value={defaultModelInput}
        onChange={(event) => setDefaultModelInput(event.target.value)}
        disabled={saving || loadingModels}
      >
        <option value="">No default model</option>
        {models.map((model) => (
          <option key={model.id} value={model.id?.toString() ?? ''}>
            {model.name}
            {model.quantization ? ` (${model.quantization})` : ''}
          </option>
        ))}
      </Select>
    </SettingField>
  </>
);
