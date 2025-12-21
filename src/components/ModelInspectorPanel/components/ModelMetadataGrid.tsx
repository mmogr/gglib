import { FC } from 'react';
import type { GgufModel } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';

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
            <button
              className="icon-btn icon-btn-sm"
              onClick={() => copyToClipboard(model.file_path)}
              title="Copy path"
            >
              📋
            </button>
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
                🤗
              </button>
            </span>
          </div>
        )}
      </div>
    </section>
  );
};
