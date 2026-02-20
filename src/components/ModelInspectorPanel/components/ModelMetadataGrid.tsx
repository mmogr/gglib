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
    <section className="mb-xl">
      <h3 className="m-0 mb-base text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Model Information</h3>
      <div className="flex flex-col gap-md">
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Size:</span>
          <span className="text-text text-sm text-right break-words">
            {formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}
          </span>
        </div>
        {model.architecture && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Architecture:</span>
            <span className="text-text text-sm text-right break-words">{model.architecture}</span>
          </div>
        )}
        {model.quantization && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Quantization:</span>
            <span className="text-text text-sm text-right break-words font-semibold text-primary">{model.quantization}</span>
          </div>
        )}
        {model.contextLength && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Context Length:</span>
            <span className="text-text text-sm text-right break-words">{model.contextLength.toLocaleString()}</span>
          </div>
        )}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Path:</span>
          <span className="text-text text-sm text-right break-words font-mono text-xs flex items-center gap-sm">
            {model.filePath}
            <Button
              variant="ghost"
              size="sm"
              onClick={() => copyToClipboard(model.filePath)}
              title="Copy path"
              iconOnly
            >
              <Icon icon={Copy} size={14} />
            </Button>
          </span>
        </div>
        {model.hfRepoId && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">HuggingFace:</span>
            <span className="text-text text-sm text-right break-words flex items-center gap-sm">
              <span className="font-mono text-sm text-text">{model.hfRepoId}</span>
              <button
                className="bg-transparent border-none cursor-pointer text-[1rem] p-[2px_4px] rounded-sm opacity-70 transition-all duration-200 shrink-0 hover:opacity-100 hover:bg-background-hover hover:scale-110 active:scale-95"
                onClick={() => {
                  const url = getHuggingFaceUrl(model.hfRepoId);
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
      {model.inferenceDefaults && Object.values(model.inferenceDefaults).some(v => v != null) && (
        <>
          <h3 className="m-0 mb-base text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Inference Defaults</h3>
          <div className="flex flex-col gap-md">
            {model.inferenceDefaults.temperature != null && (
              <div className="flex justify-between items-start gap-base">
                <span className="text-text-muted text-sm shrink-0">Temperature:</span>
                <span className="text-text text-sm text-right break-words">{model.inferenceDefaults.temperature}</span>
              </div>
            )}
            {model.inferenceDefaults.topP != null && (
              <div className="flex justify-between items-start gap-base">
                <span className="text-text-muted text-sm shrink-0">Top P:</span>
                <span className="text-text text-sm text-right break-words">{model.inferenceDefaults.topP}</span>
              </div>
            )}
            {model.inferenceDefaults.topK != null && (
              <div className="flex justify-between items-start gap-base">
                <span className="text-text-muted text-sm shrink-0">Top K:</span>
                <span className="text-text text-sm text-right break-words">{model.inferenceDefaults.topK}</span>
              </div>
            )}
            {model.inferenceDefaults.maxTokens != null && (
              <div className="flex justify-between items-start gap-base">
                <span className="text-text-muted text-sm shrink-0">Max Tokens:</span>
                <span className="text-text text-sm text-right break-words">{model.inferenceDefaults.maxTokens.toLocaleString()}</span>
              </div>
            )}
            {model.inferenceDefaults.repeatPenalty != null && (
              <div className="flex justify-between items-start gap-base">
                <span className="text-text-muted text-sm shrink-0">Repeat Penalty:</span>
                <span className="text-text text-sm text-right break-words">{model.inferenceDefaults.repeatPenalty}</span>
              </div>
            )}
          </div>
        </>
      )}
    </section>
  );
};
