import { FC } from 'react';
import type { GgufModel, ModelDetail } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { Copy, ExternalLink } from 'lucide-react';

interface ModelMetadataGridProps {
  model: GgufModel;
  /** Full model detail from GET /api/models/:id/detail. Enables HF provenance rows and GGUF metadata. */
  detail?: ModelDetail;
}

/**
 * Read-only metadata display for the model inspector.
 * Shows size, architecture, quantization, context length, path, and HuggingFace link.
 */
export const ModelMetadataGrid: FC<ModelMetadataGridProps> = ({ model, detail }) => {
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
        {/* Context Length — shows server default override or model metadata */}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Context Length:</span>
          <span className="text-text text-sm text-right break-words">
            {model.serverDefaults?.contextLength ? model.serverDefaults.contextLength.toLocaleString() : (model.contextLength ? `${model.contextLength.toLocaleString()} (default)` : 'Using default')}
          </span>
        </div>
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
        {detail?.hfFilename && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">HF File:</span>
            <span className="text-text text-xs text-right break-all font-mono">{detail.hfFilename}</span>
          </div>
        )}
        {detail?.hfCommitSha && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Commit:</span>
            <span className="text-text text-sm text-right font-mono" title={detail.hfCommitSha}>
              {detail.hfCommitSha.slice(0, 7)}
            </span>
          </div>
        )}
        {detail?.downloadDate && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Downloaded:</span>
            <span className="text-text text-sm text-right break-words">
              {new Date(detail.downloadDate).toLocaleString()}
            </span>
          </div>
        )}
        {detail?.lastUpdateCheck && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Last checked:</span>
            <span className="text-text text-sm text-right break-words">
              {new Date(detail.lastUpdateCheck).toLocaleString()}
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

      {/* Raw GGUF Metadata — stateless collapsible via native <details> */}
      {detail && Object.keys(detail.metadata).length > 0 && (
        <details className="mt-xl border-t border-border pt-base">
          <summary className="cursor-pointer text-sm font-semibold text-text-secondary uppercase tracking-[0.05em] select-none">
            Raw GGUF Metadata ({Object.keys(detail.metadata).length} keys)
          </summary>
          <div className="mt-base flex flex-col gap-md max-h-64 overflow-y-auto pr-xs">
            {Object.entries(detail.metadata)
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([key, value]) => (
                <div key={key} className="flex justify-between items-start gap-base">
                  <span className="text-text-muted text-xs font-mono shrink-0 max-w-[45%] break-all">{key}</span>
                  <span className="text-text text-xs font-mono text-right break-all">{value}</span>
                </div>
              ))}
          </div>
        </details>
      )}
    </section>
  );
};
