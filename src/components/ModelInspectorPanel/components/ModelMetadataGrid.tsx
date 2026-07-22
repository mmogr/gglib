import { FC } from 'react';
import { ChevronRight, Copy, ExternalLink } from 'lucide-react';
import type { GgufModel, ModelDetail } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { InfoRow } from './InfoRow';
import { MetadataSection } from './MetadataSection';

interface ModelMetadataGridProps {
  model: GgufModel;
  /** Full model detail from GET /api/models/:id/detail. Enables HF provenance rows and GGUF metadata. */
  detail?: ModelDetail;
}

/** Context length, preferring an explicit server override over GGUF metadata. */
function formatContextLength(model: GgufModel): string {
  if (model.serverDefaults?.contextLength) {
    return model.serverDefaults.contextLength.toLocaleString();
  }
  if (model.contextLength) {
    return `${model.contextLength.toLocaleString()} (default)`;
  }
  return 'Using default';
}

/**
 * Read-only metadata display for the model inspector.
 * Shows size, architecture, quantization, context length, path, and HuggingFace link.
 */
export const ModelMetadataGrid: FC<ModelMetadataGridProps> = ({ model, detail }) => {
  const inferenceDefaults = model.inferenceDefaults;
  const hasInferenceDefaults =
    inferenceDefaults != null && Object.values(inferenceDefaults).some((v) => v != null);
  const metadataEntries = detail ? Object.entries(detail.metadata) : [];

  return (
    <section className="mb-xl">
      <MetadataSection title="Model Information">
        <InfoRow label="Size">
          {formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}
        </InfoRow>

        {model.architecture && <InfoRow label="Architecture">{model.architecture}</InfoRow>}

        {model.quantization && (
          <InfoRow label="Quantization" className="font-semibold text-primary">
            {model.quantization}
          </InfoRow>
        )}

        <InfoRow label="Context Length">{formatContextLength(model)}</InfoRow>

        <InfoRow label="Path" mono>
          <span className="inline-flex items-start gap-sm">
            <span className="min-w-0 break-all">{model.filePath}</span>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => navigator.clipboard.writeText(model.filePath)}
              title="Copy path"
              aria-label="Copy path"
              iconOnly
            >
              <Icon icon={Copy} size={14} />
            </Button>
          </span>
        </InfoRow>

        {model.hfRepoId && (
          <InfoRow label="HuggingFace">
            <span className="inline-flex items-center gap-sm">
              <span className="font-mono min-w-0 break-all">{model.hfRepoId}</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  const url = getHuggingFaceUrl(model.hfRepoId);
                  if (url) openUrl(url);
                }}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
                iconOnly
              >
                <Icon icon={ExternalLink} size={14} />
              </Button>
            </span>
          </InfoRow>
        )}

        {detail?.hfFilename && (
          <InfoRow label="HF File" mono>
            {detail.hfFilename}
          </InfoRow>
        )}

        {detail?.hfCommitSha && (
          <InfoRow label="Commit" mono>
            <span title={detail.hfCommitSha}>{detail.hfCommitSha.slice(0, 7)}</span>
          </InfoRow>
        )}

        {detail?.downloadDate && (
          <InfoRow label="Downloaded">{new Date(detail.downloadDate).toLocaleString()}</InfoRow>
        )}

        {detail?.lastUpdateCheck && (
          <InfoRow label="Last checked">
            {new Date(detail.lastUpdateCheck).toLocaleString()}
          </InfoRow>
        )}
      </MetadataSection>

      {hasInferenceDefaults && (
        <MetadataSection title="Inference Defaults">
          {inferenceDefaults.temperature != null && (
            <InfoRow label="Temperature">{inferenceDefaults.temperature}</InfoRow>
          )}
          {inferenceDefaults.topP != null && <InfoRow label="Top P">{inferenceDefaults.topP}</InfoRow>}
          {inferenceDefaults.topK != null && <InfoRow label="Top K">{inferenceDefaults.topK}</InfoRow>}
          {inferenceDefaults.maxTokens != null && (
            <InfoRow label="Max Tokens">{inferenceDefaults.maxTokens.toLocaleString()}</InfoRow>
          )}
          {inferenceDefaults.repeatPenalty != null && (
            <InfoRow label="Repeat Penalty">{inferenceDefaults.repeatPenalty}</InfoRow>
          )}
        </MetadataSection>
      )}

      {/* Raw GGUF Metadata — stateless collapsible via native <details>.
          The native disclosure marker is suppressed in favour of a lucide
          chevron so it matches the rest of the app's iconography. */}
      {metadataEntries.length > 0 && (
        <details className="group mt-xl border-t border-border pt-base">
          <summary className="flex items-center gap-sm cursor-pointer text-xs font-semibold text-text-secondary uppercase tracking-[0.05em] select-none list-none [&::-webkit-details-marker]:hidden">
            <Icon
              icon={ChevronRight}
              size={14}
              className="transition-transform duration-200 group-open:rotate-90"
            />
            Raw GGUF Metadata ({metadataEntries.length} keys)
          </summary>
          <dl className="mt-base grid grid-cols-[minmax(0,45%)_1fr] gap-x-base gap-y-md m-0 max-h-64 overflow-y-auto pr-xs scrollbar-thin">
            {metadataEntries
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([key, value]) => (
                <InfoRow
                  key={key}
                  label={key}
                  mono
                  labelClassName="text-xs font-mono break-all"
                >
                  {value}
                </InfoRow>
              ))}
          </dl>
        </details>
      )}
    </section>
  );
};
