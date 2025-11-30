import { FC, useState, useEffect, useCallback } from 'react';
import { HfModelSummary, HfQuantization, HfQuantizationsResponse, FitStatus } from '../../types';
import { TauriService } from '../../services/tauri';
import { formatBytes, formatNumber, getHuggingFaceModelUrl } from '../../utils/format';
import { useSystemMemory } from '../../hooks/useSystemMemory';
import { useSettings } from '../../hooks/useSettings';
import styles from './HfModelPreview.module.css';

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

  const iconMap: Record<FitStatus, { icon: string; className: string }> = {
    fits: { icon: '✅', className: styles.fitIndicatorFits },
    tight: { icon: '⚠️', className: styles.fitIndicatorTight },
    wont_fit: { icon: '❌', className: styles.fitIndicatorWontFit },
  };

  const { icon, className } = iconMap[status];

  return (
    <span 
      className={`${styles.fitIndicator} ${className}`}
      title={tooltip}
      aria-label={tooltip}
    >
      {icon}
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

  // Memory fit checking
  const { checkFit, getTooltip, loading: memoryLoading } = useSystemMemory();
  const { settings } = useSettings();
  const showFitIndicators = settings?.show_memory_fit_indicators ?? true;

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
        const response: HfQuantizationsResponse = await TauriService.getHfQuantizations(model.id);
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

  const handleOpenHuggingFace = useCallback(() => {
    const url = getHuggingFaceModelUrl(model.id);
    TauriService.openUrl(url);
  }, [model.id]);

  const handleDownload = useCallback((quant: HfQuantization) => {
    onDownload(model.id, quant.name);
  }, [model.id, onDownload]);

  return (
    <div className={styles.hfModelPreview}>
      {/* Model Header */}
      <div className={styles.modelHeader}>
        <div className={styles.modelTitleRow}>
          <h2 className={styles.modelName}>{model.name}</h2>
          <button
            className={styles.hfButton}
            onClick={handleOpenHuggingFace}
            title="Open on HuggingFace"
            aria-label="Open on HuggingFace"
          >
            🤗
          </button>
        </div>
        <div className={styles.modelAuthor}>by {model.author || model.id.split('/')[0]}</div>
        
        {/* Stats row */}
        <div className={styles.statsRow}>
          {model.parameters_b && (
            <span className={styles.statBadge}>
              {model.parameters_b.toFixed(1)}B params
            </span>
          )}
          <span className={styles.stat}>
            <span className={styles.statIcon}>⬇️</span>
            {formatNumber(model.downloads)}
          </span>
          <span className={styles.stat}>
            <span className={styles.statIcon}>❤️</span>
            {formatNumber(model.likes)}
          </span>
          {model.last_modified && (
            <span className={styles.stat}>
              <span className={styles.statIcon}>📅</span>
              {formatLastModified(model.last_modified)}
            </span>
          )}
        </div>
      </div>

      {/* Quantization Table */}
      <div className={styles.quantSection}>
        <h3 className={styles.sectionTitle}>Quantization Options</h3>
        
        {loadingQuants && (
          <div className={styles.loadingState}>
            <span className={styles.spinner}></span>
            Loading quantizations...
          </div>
        )}

        {quantError && (
          <div className={styles.errorState}>{quantError}</div>
        )}

        {!loadingQuants && !quantError && quantizations.length === 0 && (
          <div className={styles.emptyState}>No GGUF quantizations found</div>
        )}

        {!loadingQuants && !quantError && quantizations.length > 0 && (
          <div className={styles.quantTable}>
            <div className={styles.quantTableHeader}>
              <span className={styles.colQuant}>Quant</span>
              <span className={styles.colSize}>Size</span>
              <span className={styles.colShards}>Shards</span>
              {showFitIndicators && !memoryLoading && (
                <span className={styles.colFit}>Fit</span>
              )}
              <span className={styles.colAction}></span>
            </div>
            <div className={styles.quantTableBody}>
              {quantizations.map((quant) => (
                <div key={quant.name} className={styles.quantRow}>
                  <span className={styles.colQuant}>
                    <span className={styles.quantName}>{quant.name}</span>
                  </span>
                  <span className={styles.colSize}>{formatBytes(quant.size_bytes)}</span>
                  <span className={styles.colShards}>
                    {quant.is_sharded ? quant.shard_count : 1}
                  </span>
                  {showFitIndicators && !memoryLoading && (
                    <span className={styles.colFit}>
                      <FitIndicator
                        sizeBytes={quant.size_bytes}
                        checkFit={checkFit}
                        getTooltip={getTooltip}
                      />
                    </span>
                  )}
                  <span className={styles.colAction}>
                    <button
                      className={styles.downloadBtn}
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
        <div className={styles.descriptionSection}>
          <h3 className={styles.sectionTitle}>Description</h3>
          <p className={styles.description}>{model.description}</p>
        </div>
      )}

      {/* Tags */}
      {model.tags && model.tags.length > 0 && (
        <div className={styles.tagsSection}>
          <h3 className={styles.sectionTitle}>Tags</h3>
          <div className={styles.tagsList}>
            {model.tags.slice(0, 10).map((tag) => (
              <span key={tag} className={styles.tag}>{tag}</span>
            ))}
            {model.tags.length > 10 && (
              <span className={styles.moreTagsIndicator}>+{model.tags.length - 10} more</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

export default HfModelPreview;
