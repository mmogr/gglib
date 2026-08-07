import { FC } from 'react';
import { ExternalLink, Undo2, X } from 'lucide-react';
import type { GgufModel, InferenceConfig, ServerConfig } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import { Input } from '../../ui/Input';
import { InferenceParametersForm } from '../../InferenceParametersForm';
import { useSamplingExplanation } from '../hooks/useSamplingExplanation';

interface ModelEditFormProps {
  model: GgufModel;
  editedQuantization: string;
  editedFilePath: string;
  editedInferenceDefaults: InferenceConfig | undefined;
  editedServerDefaults: ServerConfig | null | undefined;
  onQuantizationChange: (quant: string) => void;
  onFilePathChange: (path: string) => void;
  onInferenceDefaultsChange: (config: InferenceConfig) => void;
  onServerDefaultsChange: (config: ServerConfig | null) => void;
}

/**
 * Edit mode form for model metadata.
 * Shows editable inputs for quantization and file path,
 * with read-only display for other fields.
 */
export const ModelEditForm: FC<ModelEditFormProps> = ({
  model,
  editedQuantization,
  editedFilePath,
  editedInferenceDefaults,
  editedServerDefaults,
  onQuantizationChange,
  onFilePathChange,
  onInferenceDefaultsChange,
  onServerDefaultsChange,
}) => {
  // What the model's parameters resolve to today, so an empty field can say
  // what it will inherit instead of guessing at the floor. Keyed on the saved
  // defaults, so saving an edit re-resolves rather than leaving the captions
  // describing the previous configuration.
  const resolution = useSamplingExplanation(model.id, null, model.inferenceDefaults);

  return (
    <>
      <section className="mb-xl">
        <h3 className="m-0 mb-base text-sm font-semibold text-text">Model Information</h3>
      <div className="flex flex-col gap-md">
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Size:</span>
          <span className="text-text text-sm text-right break-words">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
        </div>
        {model.architecture && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Architecture:</span>
            <span className="text-text text-sm text-right break-words">{model.architecture}</span>
          </div>
        )}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Quantization:</span>
          <Input
            type="text"
            className="min-w-[200px] flex-1"
            value={editedQuantization}
            onChange={(e) => onQuantizationChange(e.target.value)}
            placeholder="e.g., Q4_0"
          />
        </div>
        {/* Context Length — editable override */}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Context Length:</span>
          <div className="flex items-center gap-2">
            <Input
              type="number"
              min={256}
              step={1}
              value={editedServerDefaults?.contextLength ?? ''}
              onChange={(e) => {
                const val = e.target.value ? parseInt(e.target.value, 10) : undefined;
                onServerDefaultsChange(val !== undefined ? { contextLength: val } : null);
              }}
              placeholder="Use default"
              className="w-32"
            />
            {editedServerDefaults !== undefined && (
              <IconButton
                label={editedServerDefaults === null ? "Revert 'clear' action" : "Clear override"}
                size="sm"
                onClick={() => {
                  // Toggle: null (clear) ↔ object with current model value (revert to model default)
                  if (editedServerDefaults === null) {
                    onServerDefaultsChange(model.serverDefaults ?? {});
                  } else {
                    onServerDefaultsChange(null);
                  }
                }}
              >
                <Icon icon={editedServerDefaults === null ? Undo2 : X} size={14} />
              </IconButton>
            )}
          </div>
        </div>
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Path:</span>
          <Input
            type="text"
            className="min-w-[200px] flex-1 font-mono text-xs"
            value={editedFilePath}
            onChange={(e) => onFilePathChange(e.target.value)}
            placeholder="File path"
          />
        </div>
        {model.hfRepoId && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">HuggingFace:</span>
            <span className="text-text text-sm text-right break-words flex items-center gap-sm">
              <span className="font-mono text-sm text-text">{model.hfRepoId}</span>
              <IconButton
                label="Open on HuggingFace"
                size="sm"
                className="shrink-0"
                onClick={() => {
                  const url = getHuggingFaceUrl(model.hfRepoId);
                  if (url) openUrl(url);
                }}
              >
                <Icon icon={ExternalLink} size={14} />
              </IconButton>
            </span>
          </div>
        )}
      </div>
    </section>
    
    <InferenceParametersForm
      value={editedInferenceDefaults}
      onChange={onInferenceDefaultsChange}
      fallback={{ kind: 'resolved', ownLayer: 'modelUserSet', resolution }}
    />
    </>
  );
};
