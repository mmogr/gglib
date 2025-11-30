import { useState, useCallback, useRef, useEffect, FC } from "react";
import { TauriService } from "../../services/tauri";
import {
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantization,
  HfQuantizationsResponse,
  HfSortField,
  FitStatus,
} from "../../types";
import { useDownloadProgress } from "../../hooks/useDownloadProgress";
import { useSystemMemory } from "../../hooks/useSystemMemory";
import { useSettings } from "../../hooks/useSettings";
import { DownloadProgressDisplay } from "../DownloadProgressDisplay";
import { formatBytes, formatNumber, getHuggingFaceModelUrl } from "../../utils/format";
import styles from "./HuggingFaceBrowser.module.css";

interface HuggingFaceBrowserProps {
  /** Callback when a model download is initiated */
  onDownloadStarted?: () => void;
  /** Callback when a model download completes */
  onDownloadCompleted?: () => void;
  /** Callback when a model is selected (clicked) for preview */
  onSelectModel?: (model: HfModelSummary | null) => void;
  /** Currently selected model ID (for highlighting) */
  selectedModelId?: string | null;
}

// Sort options configuration
interface SortOption {
  value: HfSortField;
  label: string;
  defaultAscending: boolean;
}

const SORT_OPTIONS: SortOption[] = [
  { value: "downloads", label: "Downloads", defaultAscending: false },
  { value: "likes", label: "Likes", defaultAscending: false },
  { value: "modified", label: "Recently Updated", defaultAscending: false },
  { value: "created", label: "Recently Created", defaultAscending: false },
  { value: "id", label: "Alphabetical", defaultAscending: true },
];

// Fit indicator component for "Will it fit?" memory check
interface FitIndicatorProps {
  sizeBytes: number;
  checkFit: (sizeBytes: number) => FitStatus;
  getTooltip: (sizeBytes: number) => string;
}

const FitIndicator: FC<FitIndicatorProps> = ({ sizeBytes, checkFit, getTooltip }) => {
  const status = checkFit(sizeBytes);
  const tooltip = getTooltip(sizeBytes);

  const iconMap: Record<FitStatus, { icon: string; className: string }> = {
    fits: { icon: "✅", className: styles.fitIndicatorFits },
    tight: { icon: "⚠️", className: styles.fitIndicatorTight },
    wont_fit: { icon: "❌", className: styles.fitIndicatorWontFit },
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

// Debounce helper
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

// Model card component with expandable quantization panel
interface ModelCardProps {
  model: HfModelSummary;
  onDownload: (modelId: string, quantization: string) => void;
  /** Callback when the model card is clicked (for preview) */
  onSelect: () => void;
  /** Whether this model is currently selected */
  isSelected: boolean;
  /** Whether download buttons should be disabled (queue full) */
  downloadsDisabled: boolean;
  /** Tooltip text when downloads are disabled */
  disabledReason?: string;
  /** Whether to show memory fit indicators */
  showFitIndicators: boolean;
  /** Check fit status for a given file size */
  checkFit: (sizeBytes: number) => FitStatus;
  /** Get tooltip text for a given file size */
  getTooltip: (sizeBytes: number) => string;
}

const ModelCard: FC<ModelCardProps> = ({ 
  model, 
  onDownload,
  onSelect,
  isSelected,
  downloadsDisabled, 
  disabledReason,
  showFitIndicators,
  checkFit,
  getTooltip,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [quantizations, setQuantizations] = useState<HfQuantization[]>([]);
  const [loadingQuants, setLoadingQuants] = useState(false);
  const [quantError, setQuantError] = useState<string | null>(null);

  const handleToggleExpand = useCallback(async () => {
    if (!expanded && quantizations.length === 0 && !loadingQuants) {
      // Load quantizations when expanding for the first time
      setLoadingQuants(true);
      setQuantError(null);
      try {
        const response: HfQuantizationsResponse =
          await TauriService.getHfQuantizations(model.id);
        setQuantizations(response.quantizations);
      } catch (err) {
        setQuantError(
          err instanceof Error ? err.message : "Failed to load quantizations"
        );
      } finally {
        setLoadingQuants(false);
      }
    }
    setExpanded(!expanded);
  }, [expanded, quantizations.length, loadingQuants, model.id]);

  const handleDownload = (quant: HfQuantization) => {
    onDownload(model.id, quant.name);
  };

  const handleOpenHuggingFace = (e: React.MouseEvent) => {
    e.stopPropagation();
    const url = getHuggingFaceModelUrl(model.id);
    TauriService.openUrl(url);
  };

  const handleCardClick = () => {
    onSelect();
    handleToggleExpand();
  };

  return (
    <div className={`${styles.modelCard} ${isSelected ? styles.modelCardSelected : ''}`}>
      <div className={styles.modelCardHeader} onClick={handleCardClick}>
        <div className={styles.modelCardMain}>
          <div className={styles.modelInfo}>
            <h3 className={styles.modelName}>
              {model.name}
              <button
                className={styles.hfButton}
                onClick={handleOpenHuggingFace}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
              >
                🤗
              </button>
            </h3>
            <span className={styles.modelId}>{model.id}</span>
            {model.description && (
              <p className={styles.modelDescription}>{model.description}</p>
            )}
          </div>
          <div className={styles.modelStats}>
            {model.parameters_b && (
              <span className={styles.paramBadge}>
                {model.parameters_b.toFixed(1)}B
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
            <span
              className={`${styles.expandIcon} ${expanded ? styles.expandIconOpen : ""}`}
            >
              ▼
            </span>
          </div>
        </div>
      </div>

      {expanded && (
        <div className={styles.quantPanel}>
          {loadingQuants && (
            <div className={styles.quantPanelLoading}>
              <span className={styles.spinner}></span>
              Loading quantizations...
            </div>
          )}

          {quantError && (
            <div className={styles.quantPanelError}>{quantError}</div>
          )}

          {!loadingQuants && !quantError && quantizations.length === 0 && (
            <div className={styles.quantPanelLoading}>
              No quantizations found
            </div>
          )}

          {!loadingQuants && !quantError && quantizations.length > 0 && (
            <div className={styles.quantGrid}>
              {quantizations.map((quant) => (
                <div key={quant.name} className={styles.quantItem}>
                  <div className={styles.quantInfo}>
                    <span className={styles.quantName}>
                      {quant.name}
                      {quant.is_sharded && (
                        <span className={styles.shardedBadge}>
                          {quant.shard_count} shards
                        </span>
                      )}
                    </span>
                    <span className={styles.quantSize}>
                      {formatBytes(quant.size_bytes)}
                    </span>
                  </div>
                  <div className={styles.quantActions}>
                    {showFitIndicators && (
                      <FitIndicator
                        sizeBytes={quant.size_bytes}
                        checkFit={checkFit}
                        getTooltip={getTooltip}
                      />
                    )}
                    <button
                      className={styles.downloadBtn}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDownload(quant);
                      }}
                      disabled={downloadsDisabled}
                      title={downloadsDisabled ? disabledReason : "Add to download queue"}
                    >
                      Download
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

// Main browser component
const HuggingFaceBrowser: FC<HuggingFaceBrowserProps> = ({
  onDownloadStarted,
  onDownloadCompleted,
  onSelectModel,
  selectedModelId,
}) => {
  // Search state
  const [searchQuery, setSearchQuery] = useState("");
  const [minParams, setMinParams] = useState("");
  const [maxParams, setMaxParams] = useState("");
  const [sortBy, setSortBy] = useState<HfSortField>("downloads");
  const [sortAscending, setSortAscending] = useState(false);

  // Results state
  const [models, setModels] = useState<HfModelSummary[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [currentPage, setCurrentPage] = useState(0);

  // Loading/error state
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Download progress - use queueStatus to determine if we can queue more downloads
  const { progress, queueStatus, queueCount, isDownloading, cancelDownload } = useDownloadProgress({
    onCompleted: () => {
      onDownloadCompleted?.();
    },
  });

  // System memory and settings for "Will it fit?" indicators
  const { checkFit, getTooltip } = useSystemMemory();
  const { settings } = useSettings();
  const showFitIndicators = settings?.show_memory_fit_indicators !== false;

  // Debounced search
  const debouncedQuery = useDebounce(searchQuery, 300);

  // Ref to track if we should fetch on debounced change
  const isInitialMount = useRef(true);

  // Handle sort change
  const handleSortChange = (newSortBy: HfSortField) => {
    const sortOption = SORT_OPTIONS.find((opt) => opt.value === newSortBy);
    setSortBy(newSortBy);
    // Set the default direction for this sort option
    if (sortOption) {
      setSortAscending(sortOption.defaultAscending);
    }
    // Trigger a new search with the updated sort
    setCurrentPage(0);
  };

  // Build search request
  const buildSearchRequest = useCallback(
    (page: number): HfSearchRequest => ({
      query: searchQuery.trim() || null,
      min_params_b: minParams ? parseFloat(minParams) : null,
      max_params_b: maxParams ? parseFloat(maxParams) : null,
      page,
      limit: 30,
      sort_by: sortBy,
      sort_ascending: sortAscending,
    }),
    [searchQuery, minParams, maxParams, sortBy, sortAscending]
  );

  // Perform search
  const performSearch = useCallback(
    async (page: number, append: boolean = false) => {
      if (page === 0) {
        setLoading(true);
      } else {
        setLoadingMore(true);
      }
      setError(null);

      try {
        const request = buildSearchRequest(page);
        const response: HfSearchResponse =
          await TauriService.browseHfModels(request);

        if (append) {
          setModels((prev) => [...prev, ...response.models]);
        } else {
          setModels(response.models);
        }
        setHasMore(response.has_more);
        setCurrentPage(response.page);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to search models"
        );
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [buildSearchRequest]
  );

  // Handle search button click
  const handleSearch = () => {
    setCurrentPage(0);
    performSearch(0, false);
  };

  // Handle load more
  const handleLoadMore = () => {
    performSearch(currentPage + 1, true);
  };

  // Handle enter key in search input
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSearch();
    }
  };

  // Handle download
  const handleDownload = async (modelId: string, quantization: string) => {
    setError(null);
    try {
      await TauriService.queueDownload(modelId, quantization);
      onDownloadStarted?.();
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to queue download"
      );
    }
  };

  // Handle cancel download
  const handleCancelDownload = async () => {
    if (progress?.model_id) {
      try {
        await cancelDownload(progress.model_id);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to cancel download"
        );
      }
    }
  };

  // Determine if queue is full (can't add more downloads)
  const maxQueueSize = queueStatus?.max_size ?? 10;
  const isQueueFull = queueCount >= maxQueueSize;
  const disabledReason = isQueueFull 
    ? `Queue is full (${queueCount}/${maxQueueSize})`
    : undefined;

  // Auto-search on debounced query change (after initial mount)
  useEffect(() => {
    if (isInitialMount.current) {
      isInitialMount.current = false;
      // Perform initial search
      performSearch(0, false);
      return;
    }

    // Don't auto-search if query is empty (user cleared input)
    // They can manually click Search to get all results
  }, [debouncedQuery]);

  // Auto-search when sort changes
  useEffect(() => {
    if (!isInitialMount.current) {
      performSearch(0, false);
    }
  }, [sortBy, sortAscending]);

  return (
    <div className={styles.container}>
      {/* Search Section */}
      <div className={styles.searchSection}>
        <div className={styles.searchRow}>
          <div className={styles.searchInputWrapper}>
            <label className={styles.searchLabel}>Search Models</label>
            <input
              type="text"
              className={styles.searchInput}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search GGUF text-generation models..."
            />
          </div>
          <button
            className={styles.searchBtn}
            onClick={handleSearch}
            disabled={loading}
          >
            {loading ? "Searching..." : "Search"}
          </button>
        </div>

        <div className={styles.filterRow}>
          <div className={styles.filterGroup}>
            <label className={styles.searchLabel}>Min Params (B)</label>
            <input
              type="number"
              className={styles.filterInput}
              value={minParams}
              onChange={(e) => setMinParams(e.target.value)}
              placeholder="e.g. 3"
              min="0"
              step="0.1"
            />
          </div>
          <div className={styles.filterGroup}>
            <label className={styles.searchLabel}>Max Params (B)</label>
            <input
              type="number"
              className={styles.filterInput}
              value={maxParams}
              onChange={(e) => setMaxParams(e.target.value)}
              placeholder="e.g. 13"
              min="0"
              step="0.1"
            />
          </div>
          <div className={styles.filterGroup}>
            <label className={styles.searchLabel}>Sort By</label>
            <div className={styles.sortWrapper}>
              <select
                className={styles.sortSelect}
                value={sortBy}
                onChange={(e) => handleSortChange(e.target.value as HfSortField)}
              >
                {SORT_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <button
                className={styles.sortDirectionBtn}
                onClick={() => setSortAscending(!sortAscending)}
                title={sortAscending ? "Ascending" : "Descending"}
              >
                {sortAscending ? "↑" : "↓"}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Results Section */}
      <div className={styles.resultsSection}>
        {/* Download Progress & Queue Status */}
        {(isDownloading || queueCount > 0) && (
          <div className={styles.downloadStatusSection}>
            {/* Queue status indicator */}
            <div className={styles.queueStatusBar}>
              <span className={styles.queueStatusText}>
                📥 {queueCount > 1 ? `${queueCount} downloads queued` : 'Downloading'}
                {isDownloading && progress && (
                  <span className={styles.currentDownload}>
                    {" "}— {progress.model_id.length > 30 
                      ? progress.model_id.substring(0, 27) + "..." 
                      : progress.model_id}
                  </span>
                )}
              </span>
            </div>
            
            {/* Download progress display */}
            {isDownloading && progress && (
              <DownloadProgressDisplay
                progress={progress}
                onCancel={handleCancelDownload}
                compact={true}
                className={styles.downloadProgress}
              />
            )}
          </div>
        )}

        {/* Error State */}
        {error && (
          <div className={styles.errorState}>
            <h4 className={styles.errorTitle}>Error</h4>
            <p className={styles.errorMessage}>{error}</p>
          </div>
        )}

        {/* Loading State */}
        {loading && (
          <div className={styles.loadingState}>
            <div className={styles.spinner}></div>
            <span>Searching HuggingFace...</span>
          </div>
        )}

        {/* Empty State */}
        {!loading && !error && models.length === 0 && (
          <div className={styles.emptyState}>
            <div className={styles.emptyIcon}>🔍</div>
            <h3 className={styles.emptyTitle}>No models found</h3>
            <p className={styles.emptyDescription}>
              Try adjusting your search query or parameter filters.
            </p>
          </div>
        )}

        {/* Results */}
        {!loading && models.length > 0 && (
          <>
            <div className={styles.resultsHeader}>
              <span className={styles.resultsCount}>
                Showing {models.length} model{models.length !== 1 ? "s" : ""}
              </span>
            </div>

            {models.map((model) => (
              <ModelCard
                key={model.id}
                model={model}
                onDownload={handleDownload}
                onSelect={() => onSelectModel?.(model)}
                isSelected={selectedModelId === model.id}
                downloadsDisabled={isQueueFull}
                disabledReason={disabledReason}
                showFitIndicators={showFitIndicators}
                checkFit={checkFit}
                getTooltip={getTooltip}
              />
            ))}

            {/* Load More Button */}
            {hasMore && (
              <div className={styles.loadMoreSection}>
                <button
                  className={styles.loadMoreBtn}
                  onClick={handleLoadMore}
                  disabled={loadingMore}
                >
                  {loadingMore ? "Loading..." : "Load More"}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
};

export default HuggingFaceBrowser;
