import { FC } from 'react';
import { ExternalLink, Undo2, X } from 'lucide-react';
import type { GgufModel, SparseInferenceConfig, ServerConfig, TemplateSupport } from '../../../types';
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
  editedInferenceDefaults: SparseInferenceConfig | undefined;
  editedServerDefaults: ServerConfig | null | undefined;
  /**
   * Whether this model's template reads `reasoning_effort`, from the model
   * detail. Absent while the detail is still loading, and on a backend that
   * predates the field — both of which are honestly `unknown`, so the form
   * offers the control and says it has not been observed.
   */
  reasoningEffortSupport?: TemplateSupport;
  onQuantizationChange: (quant: string) => void;
  onFilePathChange: (path: string) => void;
  onInferenceDefaultsChange: (config: SparseInferenceConfig) => void;
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
  reasoningEffortSupport,
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
                placeholder="No override"
                className="w-32 font-mono tabular-nums"
              />
              {editedServerDefaults !== undefined && (
                <IconButton
                  label={editedServerDefaults === null ? "Revert 'clear' action" : "Clear override"}
                  size="sm"
                  onClick={() => {
                    // Toggle: null (clear) ↔ an object, which is what puts the
                    // model's own value back.
                    //
                    // The fallback has to be a *non-null* object, and that is
                    // the whole subtlety: the only way to reach this branch is
                    // to have typed a value and then cleared it, and most
                    // models store no `serverDefaults` at all — so reverting
                    // on the ordinary model reverts to the fallback, and a
                    // `null` fallback would leave the state exactly where it
                    // was. The button would render an undo that undoes
                    // nothing, and the save would persist the clear.
                    if (editedServerDefaults === null) {
                      onServerDefaultsChange(model.serverDefaults ?? { contextLength: null });
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
      capabilities={{ reasoningEffort: reasoningEffortSupport ?? 'unknown' }}
    />
    </>
  );
};
