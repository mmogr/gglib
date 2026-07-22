import { FC } from 'react';
import { Input } from '../../ui/Input';
import { Select } from '../../ui/Select';
import type { GgufModel } from '../../../types';
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
    <SettingField
      id="context-size-input"
      label="Default Context Size"
      defaultHint="4096"
      description="Default context size for models (e.g., 4096, 8192, 16384)"
    >
      <Input
        id="context-size-input"
        type="number"
        value={contextSizeInput}
        onChange={(event) => setContextSizeInput(event.target.value)}
        placeholder="4096"
        min="512"
        max="1000000"
        disabled={saving}
      />
    </SettingField>

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
