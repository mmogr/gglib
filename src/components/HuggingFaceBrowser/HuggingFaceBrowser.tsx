import { FC } from "react";
import { HfModelSummary, HfSortField } from "../../types";
import { ModelCard } from "./components/ModelCard";
import { useHuggingFaceSearch, SORT_OPTIONS } from "./hooks/useHuggingFaceSearch";
import styles from "./HuggingFaceBrowser.module.css";

interface HuggingFaceBrowserProps {
  /** Callback when a model is selected (clicked) for preview */
  onSelectModel?: (model: HfModelSummary | null) => void;
  /** Currently selected model ID (for highlighting) */
  selectedModelId?: string | null;
}

/**
 * HuggingFace model browser component.
 * 
 * Allows searching, filtering, and browsing GGUF models from HuggingFace.
 * Supports direct download via `user/repo:quant` syntax.
 */
const HuggingFaceBrowser: FC<HuggingFaceBrowserProps> = ({
  onSelectModel,
  selectedModelId,
}) => {
  const {
    // Search input state
    searchQuery,
    setSearchQuery,
    minParams,
    setMinParams,
    maxParams,
    setMaxParams,
    sortBy,
    sortAscending,
    handleSortChange,
    setSortAscending,

    // Results state
    models,
    hasMore,

    // Loading/error state
    loading,
    loadingMore,
    error,
    searchError,

    // Search intent
    buttonText,
    buttonVariant,

    // Actions
    handleSearchAction,
    handleLoadMore,
    handleKeyDown,
  } = useHuggingFaceSearch({ onSelectModel });

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
