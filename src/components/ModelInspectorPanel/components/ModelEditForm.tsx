import { FC } from 'react';
import { ExternalLink, Undo2, X } from 'lucide-react';
import type { GgufModel, InferenceConfig, ServerConfig } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import { Input } from '../../ui/Input';
import { InfoRow } from './InfoRow';
import { MetadataSection } from './MetadataSection';
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
        <MetadataSection title="Model Information">
          <InfoRow label="Size" className="font-mono tabular-nums">
            {formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}
          </InfoRow>
          {model.architecture && <InfoRow label="Architecture">{model.architecture}</InfoRow>}
          <InfoRow label="Quantization">
            <Input
              type="text"
              className="w-full font-mono tabular-nums"
              value={editedQuantization}
              onChange={(e) => onQuantizationChange(e.target.value)}
              placeholder="e.g., Q4_0"
            />
          </InfoRow>
          <InfoRow label="Context Length">
            <span className="flex items-center gap-sm">
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
                className="w-32 font-mono tabular-nums"
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
            </span>
          </InfoRow>
          <InfoRow label="Path">
            <Input
              type="text"
              className="w-full font-mono text-xs"
              value={editedFilePath}
              onChange={(e) => onFilePathChange(e.target.value)}
              placeholder="File path"
            />
          </InfoRow>
          {model.hfRepoId && (
            <InfoRow label="HuggingFace">
              <span className="flex items-center gap-sm">
                <span className="font-mono text-sm text-text break-all">{model.hfRepoId}</span>
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
            </InfoRow>
          )}
        </MetadataSection>
    </section>
    
    <InferenceParametersForm
      value={editedInferenceDefaults}
      onChange={onInferenceDefaultsChange}
      fallback={{ kind: 'resolved', ownLayer: 'modelUserSet', resolution }}
    />
    </>
  );
};
