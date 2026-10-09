import { FC, type ReactNode } from 'react';
import { ChevronRight, Copy, ExternalLink } from 'lucide-react';
import type { GgufModel, ModelDetail } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { parseDbTimestamp } from '../../../utils/dbTimestamp';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { InfoRow } from './InfoRow';
import { MetadataSection } from './MetadataSection';

/**
 * What the grid reads of a model: a row of this library, or a model's
 * detail as the paired machine sends it, which has no path on its disk.
 */
export type MetadataModel = Pick<ModelDetail, 'paramCountB' | 'expertCount' | 'expertUsedCount' | 'filePath'> & {
  architecture?: string | null;
  quantization?: string | null;
  contextLength?: number | null;
  hfRepoId?: string | null;
  serverDefaults?: GgufModel['serverDefaults'];
};

interface ModelMetadataGridProps {
  model: MetadataModel;
  /** Full model detail from GET /api/models/:id/detail. Enables HF provenance rows and GGUF metadata. */
  detail?: ModelDetail;
  /**
   * The resolved sampling section, between the information and the raw
   * metadata. Only a model of this machine's has one: it is read from this
   * machine's daemon by the model's id here.
   */
  sampling?: ReactNode;
  /**
   * The projector row, after the path. Only a model of this machine's has
   * one: the link is a path on this machine's disk.
   */
  projector?: ReactNode;
  /**
   * The image model's components row, after the projector. Only a model of
   * this machine's has one, for the projector's reason.
   */
  components?: ReactNode;
}

/**
 * Context length, preferring an explicit server override over GGUF metadata.
 *
 * The GGUF figure is labelled `(trained)` and not `(default)`: it is the
 * window the model was trained for, and nothing serves it by default. With no
 * per-model override and nothing configured, the server sizes the launch
 * itself — fitting a window to this machine where it can read the device, and
 * falling to the floor where it cannot. See ADR 0009, and `contextPlaceholder`,
 * which is where "what will a serve actually use" is answered.
 */
export function formatContextLength(model: MetadataModel): string {
  if (model.serverDefaults?.contextLength) {
    return model.serverDefaults.contextLength.toLocaleString();
  }
  if (model.contextLength) {
    return `${model.contextLength.toLocaleString()} (trained)`;
  }
  return 'Not recorded';
}

/**
 * Read-only metadata display for the model inspector.
 * Shows size, architecture, quantization, context length, path (where the
 * model has one here), and HuggingFace link.
 */
export const ModelMetadataGrid: FC<ModelMetadataGridProps> = ({
  model,
  detail,
  sampling,
  projector,
  components,
}) => {
  const metadataEntries = detail ? Object.entries(detail.metadata) : [];

  return (
    <section className="mb-xl">
      <MetadataSection title="Model Information">
        <InfoRow label="Size" className="font-mono tabular-nums">
          {formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}
        </InfoRow>

        {model.architecture && <InfoRow label="Architecture">{model.architecture}</InfoRow>}

        {model.quantization && (
          <InfoRow label="Quantization" className="font-mono tabular-nums">
            {model.quantization}
          </InfoRow>
        )}

        <InfoRow label="Context Length" className="font-mono tabular-nums">{formatContextLength(model)}</InfoRow>

        {model.filePath && (
          <InfoRow label="Path" mono>
            <span className="inline-flex items-start gap-sm">
              <span className="min-w-0 break-all">{model.filePath}</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => model.filePath && navigator.clipboard.writeText(model.filePath)}
                title="Copy path"
                aria-label="Copy path"
                iconOnly
              >
                <Icon icon={Copy} size={14} />
              </Button>
            </span>
          </InfoRow>
        )}

        {projector}

        {components}

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
          <InfoRow label="Downloaded">{parseDbTimestamp(detail.downloadDate).toLocaleString()}</InfoRow>
        )}

        {detail?.lastUpdateCheck && (
          <InfoRow label="Last checked">
            {parseDbTimestamp(detail.lastUpdateCheck).toLocaleString()}
          </InfoRow>
        )}
      </MetadataSection>

      {sampling}

      {/* Raw GGUF Metadata — stateless collapsible via native <details>.
          The native disclosure marker is suppressed in favour of a lucide
          chevron so it matches the rest of the app's iconography. */}
      {metadataEntries.length > 0 && (
        <details className="group mt-xl border-t border-border pt-base">
          <summary className="flex items-center gap-sm cursor-pointer text-sm font-semibold text-text select-none list-none [&::-webkit-details-marker]:hidden">
            <Icon
              icon={ChevronRight}
              size={14}
              className="transition-transform duration-200 group-open:rotate-90"
            />
            Raw GGUF Metadata ({metadataEntries.length} keys)
          </summary>
          <dl className="mt-base grid grid-cols-[minmax(0,45%)_1fr] gap-x-base gap-y-md m-0 max-h-64 overflow-y-auto pr-xs">
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
