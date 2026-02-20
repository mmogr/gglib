import { FC, useState, useEffect, useCallback } from 'react';
import { appLogger } from '../../services/platform';
import {
  AlertTriangle,
  CalendarClock,
  CheckCircle2,
  Download,
  ExternalLink,
  Heart,
  HelpCircle,
  Info,
  Wrench,
  XCircle,
} from 'lucide-react';
import { HfModelSummary, HfQuantization, HfQuantizationsResponse, HfToolSupportResponse, FitStatus } from '../../types';
import { getHfQuantizations, getHfToolSupport } from '../../services/clients/huggingface';
import { openUrl } from '../../services/platform';
import { formatBytes, formatNumber, getHuggingFaceModelUrl } from '../../utils/format';
import { useSystemMemory } from '../../hooks/useSystemMemory';
import { useSettings } from '../../hooks/useSettings';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';

interface HfModelPreviewProps {
  /** The selected HuggingFace model to preview */
  model: HfModelSummary;
  /** Callback when a download is initiated */
  onDownload: (modelId: string, quantization: string) => void;
  /** Whether download buttons should be disabled (queue full) */
  downloadsDisabled?: boolean;
  /** Tooltip text when downloads are disabled */
  disabledReason?: string;
}

// Fit indicator component
interface FitIndicatorProps {
  sizeBytes: number;
  checkFit: (sizeBytes: number) => FitStatus;
  getTooltip: (sizeBytes: number) => string;
}

const FitIndicator: FC<FitIndicatorProps> = ({ sizeBytes, checkFit, getTooltip }) => {
  const status = checkFit(sizeBytes);
  const tooltip = getTooltip(sizeBytes);

  const iconMap: Record<FitStatus, { icon: typeof CheckCircle2; className: string }> = {
    fits: { icon: CheckCircle2, className: '' },
    tight: { icon: AlertTriangle, className: '' },
    wont_fit: { icon: XCircle, className: '' },
    unknown: { icon: HelpCircle, className: 'grayscale opacity-60' },
  };

  const { icon, className } = iconMap[status];

  return (
    <span 
      className={cn('text-base cursor-help', className)}
      title={tooltip}
      aria-label={tooltip}
    >
      <Icon icon={icon} size={14} />
    </span>
  );
};

/**
 * HuggingFace model preview component.
 * Displays model info, stats, quantization options with memory fit indicators,
 * and download buttons. Replaces the iframe-based preview.
 */
const HfModelPreview: FC<HfModelPreviewProps> = ({
  model,
  onDownload,
  downloadsDisabled = false,
  disabledReason,
}) => {
  const [quantizations, setQuantizations] = useState<HfQuantization[]>([]);
  const [loadingQuants, setLoadingQuants] = useState(true);
  const [quantError, setQuantError] = useState<string | null>(null);
  
  // Tool support detection state
  const [toolSupport, setToolSupport] = useState<HfToolSupportResponse | null>(null);
  const [loadingToolSupport, setLoadingToolSupport] = useState(true);

  // Memory fit checking
  const { checkFit, getTooltip, loading: memoryLoading } = useSystemMemory();
  const { settings } = useSettings();
  const showFitIndicators = settings?.showMemoryFitIndicators ?? true;

  // Format last modified date
  const formatLastModified = (dateStr?: string | null): string => {
    if (!dateStr) return 'Unknown';
    try {
      const date = new Date(dateStr);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
      
      if (diffDays === 0) return 'Today';
      if (diffDays === 1) return 'Yesterday';
      if (diffDays < 7) return `${diffDays} days ago`;
      if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
      if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
      return `${Math.floor(diffDays / 365)} years ago`;
    } catch {
      return dateStr;
    }
  };

  // Load quantizations when model changes
  useEffect(() => {
    let cancelled = false;
    
    const loadQuantizations = async () => {
      setLoadingQuants(true);
      setQuantError(null);
      
      try {
        const response: HfQuantizationsResponse = await getHfQuantizations(model.id);
        if (!cancelled) {
          // Sort by size ascending (smallest first)
          const sorted = [...response.quantizations].sort((a, b) => a.size_bytes - b.size_bytes);
          setQuantizations(sorted);
        }
      } catch (err) {
        if (!cancelled) {
          setQuantError(err instanceof Error ? err.message : 'Failed to load quantizations');
        }
      } finally {
        if (!cancelled) {
          setLoadingQuants(false);
        }
      }
    };

    loadQuantizations();
    
    return () => {
      cancelled = true;
    };
  }, [model.id]);

  // Load tool support info when model changes (parallel with quantizations)
  useEffect(() => {
    let cancelled = false;
    
    const loadToolSupport = async () => {
      setLoadingToolSupport(true);
      setToolSupport(null);
      
      try {
        const response = await getHfToolSupport(model.id);
        if (!cancelled) {
          setToolSupport(response);
        }
      } catch (err) {
        // Silently fail - tool support is optional info
        appLogger.debug('component.model', 'Failed to load tool support info', { error: err, modelId: model.id });
      } finally {
        if (!cancelled) {
          setLoadingToolSupport(false);
        }
      }
    };

    loadToolSupport();
    
    return () => {
      cancelled = true;
    };
  }, [model.id]);

  const handleOpenHuggingFace = useCallback(() => {
    const url = getHuggingFaceModelUrl(model.id);
    openUrl(url);
  }, [model.id]);

  const handleDownload = useCallback((quant: HfQuantization) => {
    onDownload(model.id, quant.name);
  }, [model.id, onDownload]);

  // Build tooltip for tool support badge
  const getToolSupportTooltip = (): string => {
    if (!toolSupport) return '';
    const formatPart = toolSupport.detected_format 
      ? ` (${toolSupport.detected_format} format)` 
      : '';
    return `Supports tool/function calling${formatPart}`;
  };

  return (
    <div className="flex flex-col gap-lg h-full overflow-y-auto p-base">
      {/* Model Header */}
      <div className="flex flex-col gap-sm pb-base border-b border-border">
        <div className="flex items-center justify-between gap-md">
          <h2 className="m-0 text-xl font-semibold text-text overflow-hidden text-ellipsis whitespace-nowrap flex-1">{model.name}</h2>
          <button
            className="shrink-0 bg-transparent border-none text-[1.25rem] cursor-pointer px-sm py-xs rounded-base transition-colors duration-150 ease-linear hover:bg-surface-hover"
            onClick={handleOpenHuggingFace}
            title="Open on HuggingFace"
            aria-label="Open on HuggingFace"
          >
            <Icon icon={ExternalLink} size={16} />
          </button>
        </div>
        <div className="text-sm text-text-secondary">by {model.author || model.id.split('/')[0]}</div>
        
        {/* Stats row */}
        <div className="flex flex-wrap gap-md items-center mt-sm">
          {model.parameters_b && (
            <span className="bg-primary text-white px-sm py-xs rounded-base text-xs font-semibold">
              {model.parameters_b.toFixed(1)}B params
            </span>
          )}
          {/* Tool support badge - only show when loaded and supported */}
          {!loadingToolSupport && toolSupport?.supports_tool_calling && (
            <span 
              className="inline-flex items-center gap-1 px-sm py-xs bg-[rgba(37,99,235,0.15)] text-primary-light rounded-base text-xs font-medium cursor-help transition-colors duration-150 ease-linear hover:bg-[rgba(37,99,235,0.25)]"
              title={getToolSupportTooltip()}
            >
              <span aria-hidden>
                <Icon icon={Wrench} size={14} />
              </span>
              <span>Tools</span>
              <span className="text-[0.7rem] opacity-70 ml-[0.15rem]" aria-hidden="true">
                <Icon icon={Info} size={12} />
              </span>
            </span>
          )}
          <span className="flex items-center gap-xs text-sm text-text-secondary">
            <span className="text-sm" aria-hidden>
              <Icon icon={Download} size={14} />
            </span>
            {formatNumber(model.downloads)}
          </span>
          <span className="flex items-center gap-xs text-sm text-text-secondary">
            <span className="text-sm" aria-hidden>
              <Icon icon={Heart} size={14} />
            </span>
            {formatNumber(model.likes)}
          </span>
          {model.last_modified && (
            <span className="flex items-center gap-xs text-sm text-text-secondary">
              <span className="text-sm" aria-hidden>
                <Icon icon={CalendarClock} size={14} />
              </span>
              {formatLastModified(model.last_modified)}
            </span>
          )}
        </div>
      </div>

      {/* Quantization Table */}
      <div className="flex flex-col gap-md">
        <h3 className="m-0 text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Quantization Options</h3>
        
        {loadingQuants && (
          <div className="p-lg text-center text-text-secondary bg-surface-elevated rounded-lg">
            <span className="inline-block w-4 h-4 border-2 border-border border-t-primary rounded-full animate-spin-360 mr-sm"></span>
            Loading quantizations...
          </div>
        )}

        {quantError && (
          <div className="p-lg text-center text-error bg-[rgba(239,68,68,0.1)] rounded-lg">{quantError}</div>
        )}

        {!loadingQuants && !quantError && quantizations.length === 0 && (
          <div className="p-lg text-center text-text-secondary bg-surface-elevated rounded-lg">No GGUF quantizations found</div>
        )}

        {!loadingQuants && !quantError && quantizations.length > 0 && (
          <div className="flex flex-col border border-border rounded-lg overflow-hidden bg-surface">
            <div className="grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md bg-surface-elevated text-xs font-semibold text-text-secondary uppercase tracking-[0.05em]">
              <span>Quant</span>
              <span>Size</span>
              <span>Shards</span>
              {showFitIndicators && !memoryLoading && (
                <span>Fit</span>
              )}
              <span></span>
            </div>
            <div className="flex flex-col max-h-[300px] overflow-y-auto">
              {quantizations.map((quant) => (
                <div key={quant.name} className="grid grid-cols-[1fr_80px_60px_50px_90px] gap-sm px-base py-md items-center border-b border-border-light last:border-b-0 transition-colors duration-150 ease-linear hover:bg-surface-hover">
                  <span className="overflow-hidden text-ellipsis whitespace-nowrap">
                    <span className="font-medium text-text">{quant.name}</span>
                  </span>
                  <span className="text-sm text-text-secondary text-right">{formatBytes(quant.size_bytes)}</span>
                  <span className="text-sm text-text-secondary text-center">
                    {quant.is_sharded ? quant.shard_count : 1}
                  </span>
                  {showFitIndicators && !memoryLoading && (
                    <span className="text-center">
                      <FitIndicator
                        sizeBytes={quant.size_bytes}
                        checkFit={checkFit}
                        getTooltip={getTooltip}
                      />
                    </span>
                  )}
                  <span className="text-right">
                    <button
                      className="px-md py-xs text-xs font-medium text-white bg-primary border-none rounded-base cursor-pointer transition-[background-color,opacity] duration-150 ease-linear hover:not-disabled:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed"
                      onClick={() => handleDownload(quant)}
                      disabled={downloadsDisabled}
                      title={downloadsDisabled ? disabledReason : `Download ${quant.name}`}
                    >
                      Download
                    </button>
                  </span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Model Description */}
      {model.description && (
        <div className="flex flex-col gap-sm">
          <h3 className="m-0 text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Description</h3>
          <p className="m-0 text-sm leading-relaxed text-text-secondary">{model.description}</p>
        </div>
      )}

      {/* Tags */}
      {model.tags && model.tags.length > 0 && (
        <div className="flex flex-col gap-sm">
          <h3 className="m-0 text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Tags</h3>
          <div className="flex flex-wrap gap-sm">
            {model.tags.slice(0, 10).map((tag) => (
              <span key={tag} className="px-sm py-xs text-xs text-text-secondary bg-surface-elevated rounded-base">{tag}</span>
            ))}
            {model.tags.length > 10 && (
              <span className="px-sm py-xs text-xs text-text-muted italic">+{model.tags.length - 10} more</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

export default HfModelPreview;
