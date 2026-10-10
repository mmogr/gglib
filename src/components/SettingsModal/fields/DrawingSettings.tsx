import { FC } from 'react';
import { Select } from '../../ui/Select';
import type { GgufModel } from '../../../types';
import type { DrawingSettingsValues } from '../useDrawingSettings';
import { SettingField } from './SettingField';
import { ToggleField } from './ToggleField';

interface DrawingSettingsProps {
  values: DrawingSettingsValues;
  onChange: <K extends keyof DrawingSettingsValues>(
    key: K,
    value: DrawingSettingsValues[K],
  ) => void;
  models: GgufModel[];
  loadingModels: boolean;
  saving: boolean;
}

/**
 * The default image model and the `/mcp` drawing switch.
 *
 * The picker lists only models that draw, since the server refuses any
 * other. A stored id the library no longer holds as an image model keeps an
 * option of its own, so opening the dialog never changes it unasked.
 */
export const DrawingSettings: FC<DrawingSettingsProps> = ({
  values,
  onChange,
  models,
  loadingModels,
  saving,
}) => {
  const imageModels = models.filter((model) => model.imageFamily != null);
  const stored = values.defaultImageModel;
  const strayStored = stored !== '' && !imageModels.some((model) => model.id?.toString() === stored);

  return (
    <div className="flex flex-col gap-md">
      <SettingField
        id="default-image-model-select"
        label="Default Image Model"
        description="The model a drawing uses when it names none. Only models that draw are listed."
      >
        <Select
          id="default-image-model-select"
          value={stored}
          onChange={(event) => onChange('defaultImageModel', event.target.value)}
          disabled={saving || loadingModels}
        >
          <option value="">No default image model</option>
          {strayStored && <option value={stored}>Model {stored} (not an image model in this library)</option>}
          {imageModels.map((model) => (
            <option key={model.id} value={model.id?.toString() ?? ''}>
              {model.name}
              {model.quantization ? ` (${model.quantization})` : ''}
            </option>
          ))}
        </Select>
      </SettingField>

      <ToggleField
        id="mcp-drawing-input"
        label="Allow MCP clients to draw"
        checked={values.mcpDrawing}
        onChange={(value) => onChange('mcpDrawing', value)}
        disabled={saving}
      >
        Offers clients of the proxy&apos;s <code>/mcp</code> endpoint gglib&apos;s drawing tool. Off by
        default: a drawing holds the GPU for minutes.
      </ToggleField>
    </div>
  );
};
