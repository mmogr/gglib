import { useState, useCallback, useRef, useEffect, useMemo, FC } from "react";
import { browseHfModels, openUrl } from "../../services/tauri";
import {
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfSortField,
} from "../../types";
import { formatNumber, getHuggingFaceModelUrl } from "../../utils/format";
import { useToolSupportCache } from "../../hooks/useToolSupportCache";
import {
  parseModelSearchIntent,
  getButtonTextForIntent,
  getButtonVariantForIntent,
} from "../../utils/modelSearchParser";
import { queueDownload } from "../../download";
import { useToast } from "../../hooks/useToast";
import styles from "./HuggingFaceBrowser.module.css";

interface HuggingFaceBrowserProps {
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

// Simplified model card - just displays info, click to select for right pane preview
interface ModelCardProps {
  model: HfModelSummary;
  /** Callback when the model card is clicked (for preview) */
  onSelect: () => void;
  /** Whether this model is currently selected */
  isSelected: boolean;
}

const ModelCard: FC<ModelCardProps> = ({ 
  model, 
  onSelect,
  isSelected,
}) => {
  // Lazy-load tool support detection (fires immediately, cached across renders)
  const { supports: supportsTools } = useToolSupportCache(model.id);

  const handleOpenHuggingFace = (e: React.MouseEvent) => {
    e.stopPropagation();
    const url = getHuggingFaceModelUrl(model.id);
    openUrl(url);
  };

  return (
    <div 
      className={`${styles.modelCard} ${isSelected ? styles.modelCardSelected : ''}`}
      onClick={onSelect}
    >
      <div className={styles.modelCardHeader}>
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
          </div>
          <div className={styles.modelStats}>
            {model.parameters_b && (
              <span className={styles.paramBadge}>
                {model.parameters_b.toFixed(1)}B
              </span>
            )}
            {supportsTools && (
              <span 
                className={styles.toolIcon}
                title="This model likely supports tool/function calling"
              >
                🔧
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
          </div>
        </div>
      </div>
    </div>
  );
};

// Main browser component
const HuggingFaceBrowser: FC<HuggingFaceBrowserProps> = ({
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
  const [searchError, setSearchError] = useState<string | null>(null);

  // Toast for notifications
  const { showToast } = useToast();

  // Compute search intent from current query
  const searchIntent = useMemo(
    () => parseModelSearchIntent(searchQuery),
    [searchQuery]
  );

  // Clear search error on query change
  useEffect(() => {
    if (searchError) {
      setSearchError(null);
    }
  }, [searchQuery]);

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
          await browseHfModels(request);

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

  // Handle search button click - now handles all intents
  const handleSearch = () => {
    setCurrentPage(0);
    performSearch(0, false);
  };

  // Handle direct download for exact repo:quant pattern
  const handleDirectDownload = useCallback(async (repo: string, quant: string) => {
    try {
      setLoading(true);
      await queueDownload(repo, quant);
      showToast(`Download started: ${repo} (${quant})`, "success");
      setSearchQuery(""); // Clear search after successful queue
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to start download";
      setSearchError(message);
      showToast(message, "error");
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  // Handle view model for exact repo pattern - fetch and select
  const handleViewRepo = useCallback(async (repo: string) => {
    try {
      setLoading(true);
      // Search for the exact repo to get model summary
      const request: HfSearchRequest = {
        query: repo,
        min_params_b: null,
        max_params_b: null,
        page: 0,
        limit: 10,
        sort_by: "downloads",
        sort_ascending: false,
      };
      const response = await browseHfModels(request);
      // Find exact match
      const exactMatch = response.models.find(m => m.id === repo);
      if (exactMatch) {
        onSelectModel?.(exactMatch);
        setSearchQuery(""); // Clear search after selecting
      } else {
        setSearchError(`Model "${repo}" not found on HuggingFace`);
        showToast(`Model "${repo}" not found`, "error");
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to fetch model";
      setSearchError(message);
      showToast(message, "error");
    } finally {
      setLoading(false);
    }
  }, [onSelectModel, showToast]);

  // Unified action handler based on current intent
  const handleSearchAction = useCallback(() => {
    switch (searchIntent.kind) {
      case "download":
        handleDirectDownload(searchIntent.repo, searchIntent.quant);
        break;
      case "repo":
        handleViewRepo(searchIntent.repo);
        break;
      case "url":
        if (searchIntent.quant && searchIntent.repo) {
          handleDirectDownload(searchIntent.repo, searchIntent.quant);
        } else if (searchIntent.repo) {
          handleViewRepo(searchIntent.repo);
        } else {
          handleSearch();
        }
        break;
      case "search":
      default:
        handleSearch();
        break;
    }
  }, [searchIntent, handleDirectDownload, handleViewRepo, handleSearch]);

  // Handle load more
  const handleLoadMore = () => {
    performSearch(currentPage + 1, true);
  };

  // Handle enter key in search input - uses unified action handler
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleSearchAction();
    }
  };

  // Get button text and variant based on intent
  const buttonText = loading ? "Loading..." : getButtonTextForIntent(searchIntent);
  const buttonVariant = getButtonVariantForIntent(searchIntent);

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
              className={`${styles.searchInput} ${searchError ? styles.searchInputError : ""}`}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search, paste user/repo, or user/repo:quant..."
            />
            {searchError && (
              <span className={styles.searchErrorText}>{searchError}</span>
            )}
          </div>
          <button
            className={`${styles.searchBtn} ${buttonVariant === "accent" ? styles.searchBtnAccent : ""} ${buttonVariant === "primary" ? styles.searchBtnPrimary : ""}`}
            onClick={handleSearchAction}
            disabled={loading}
            aria-label={buttonText}
          >
            {buttonText}
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
                onSelect={() => onSelectModel?.(model)}
                isSelected={selectedModelId === model.id}
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
