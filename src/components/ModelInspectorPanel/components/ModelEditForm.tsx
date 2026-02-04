import { FC } from 'react';
import { ExternalLink } from 'lucide-react';
import type { GgufModel, InferenceConfig } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { InferenceParametersForm } from '../../InferenceParametersForm';

interface ModelEditFormProps {
  model: GgufModel;
  editedQuantization: string;
  editedFilePath: string;
  editedInferenceDefaults: InferenceConfig | undefined;
  onQuantizationChange: (quant: string) => void;
  onFilePathChange: (path: string) => void;
  onInferenceDefaultsChange: (config: InferenceConfig) => void;
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
  onQuantizationChange,
  onFilePathChange,
  onInferenceDefaultsChange,
}) => {

  return (
    <>
      <section className="inspector-section">
        <h3>Model Information</h3>
      <div className="metadata-grid">
        <div className="metadata-row">
          <span className="metadata-label">Size:</span>
          <span className="metadata-value">{formatParamCount(model.param_count_b)}</span>
        </div>
        {model.architecture && (
          <div className="metadata-row">
            <span className="metadata-label">Architecture:</span>
            <span className="metadata-value">{model.architecture}</span>
          </div>
        )}
        <div className="metadata-row">
          <span className="metadata-label">Quantization:</span>
          <Input
            type="text"
            className="metadata-value-edit"
            value={editedQuantization}
            onChange={(e) => onQuantizationChange(e.target.value)}
            placeholder="e.g., Q4_0"
          />
        </div>
        {model.context_length && (
          <div className="metadata-row">
            <span className="metadata-label">Context Length:</span>
            <span className="metadata-value">{model.context_length.toLocaleString()}</span>
          </div>
        )}
        <div className="metadata-row">
          <span className="metadata-label">Path:</span>
          <Input
            type="text"
            className="metadata-value-edit path-edit"
            value={editedFilePath}
            onChange={(e) => onFilePathChange(e.target.value)}
            placeholder="File path"
          />
        </div>
        {model.hf_repo_id && (
          <div className="metadata-row">
            <span className="metadata-label">HuggingFace:</span>
            <span className="metadata-value hf-link-container">
              <span className="hf-repo-id">{model.hf_repo_id}</span>
              <button
                className="hf-link-button"
                onClick={() => {
                  const url = getHuggingFaceUrl(model.hf_repo_id);
                  if (url) openUrl(url);
                }}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
              >
                <Icon icon={ExternalLink} size={14} />
              </button>
            </span>
          </div>
        )}
      </div>
    </section>
    
    <InferenceParametersForm
      value={editedInferenceDefaults}
      onChange={onInferenceDefaultsChange}
    />
    </>
  );
};
