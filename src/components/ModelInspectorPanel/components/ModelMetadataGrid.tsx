import { FC } from 'react';
import type { GgufModel } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { Copy, ExternalLink } from 'lucide-react';

interface ModelMetadataGridProps {
  model: GgufModel;
}

/**
 * Read-only metadata display for the model inspector.
 * Shows size, architecture, quantization, context length, path, and HuggingFace link.
 */
export const ModelMetadataGrid: FC<ModelMetadataGridProps> = ({ model }) => {
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  return (
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
        {model.quantization && (
          <div className="metadata-row">
            <span className="metadata-label">Quantization:</span>
            <span className="metadata-value quantization">{model.quantization}</span>
          </div>
        )}
        {model.context_length && (
          <div className="metadata-row">
            <span className="metadata-label">Context Length:</span>
            <span className="metadata-value">{model.context_length.toLocaleString()}</span>
          </div>
        )}
        <div className="metadata-row">
          <span className="metadata-label">Path:</span>
          <span className="metadata-value path">
            {model.file_path}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => copyToClipboard(model.file_path)}
              title="Copy path"
              iconOnly
            >
              <Icon icon={Copy} size={14} />
            </Button>
          </span>
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

      {/* Show inference defaults if any are set */}
      {model.inferenceDefaults && Object.values(model.inferenceDefaults).some(v => v !== undefined) && (
        <>
          <h3>Inference Defaults</h3>
          <div className="metadata-grid">
            {model.inferenceDefaults.temperature !== undefined && (
              <div className="metadata-row">
                <span className="metadata-label">Temperature:</span>
                <span className="metadata-value">{model.inferenceDefaults.temperature}</span>
              </div>
            )}
            {model.inferenceDefaults.topP !== undefined && (
              <div className="metadata-row">
                <span className="metadata-label">Top P:</span>
                <span className="metadata-value">{model.inferenceDefaults.topP}</span>
              </div>
            )}
            {model.inferenceDefaults.topK !== undefined && (
              <div className="metadata-row">
                <span className="metadata-label">Top K:</span>
                <span className="metadata-value">{model.inferenceDefaults.topK}</span>
              </div>
            )}
            {model.inferenceDefaults.maxTokens !== undefined && (
              <div className="metadata-row">
                <span className="metadata-label">Max Tokens:</span>
                <span className="metadata-value">{model.inferenceDefaults.maxTokens.toLocaleString()}</span>
              </div>
            )}
            {model.inferenceDefaults.repeatPenalty !== undefined && (
              <div className="metadata-row">
                <span className="metadata-label">Repeat Penalty:</span>
                <span className="metadata-value">{model.inferenceDefaults.repeatPenalty}</span>
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
};
