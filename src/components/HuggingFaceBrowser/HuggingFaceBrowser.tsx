import { FC } from "react";
import { HfModelSummary, HfSortField } from "../../types";
import { ModelCard } from "./components/ModelCard";
import { useHuggingFaceSearch, SORT_OPTIONS } from "./hooks/useHuggingFaceSearch";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Stack, Row, EmptyState } from "../primitives";
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
    <Stack gap="base" className={styles.container}>
      {/* Search Section */}
      <Stack gap="sm" className={styles.searchSection}>
        <Row gap="sm" className={styles.searchRow} align="end">
          <Stack gap="xs" className={styles.searchInputWrapper}>
            <label className={styles.searchLabel}>Search Models</label>
            <Input
              type="text"
              className={styles.searchInput}
              variant={searchError ? "error" : "default"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search, paste user/repo, or user/repo:quant..."
            />
            {searchError && (
              <span className={styles.searchErrorText}>{searchError}</span>
            )}
          </Stack>
          <button
            className={`${styles.searchBtn} ${buttonVariant === "accent" ? styles.searchBtnAccent : ""} ${buttonVariant === "primary" ? styles.searchBtnPrimary : ""}`}
            onClick={handleSearchAction}
            disabled={loading}
            aria-label={buttonText}
          >
            {buttonText}
          </button>
        </Row>

        <Row gap="base" className={styles.filterRow} wrap>
          <Stack gap="xs" className={styles.filterGroup}>
            <label className={styles.searchLabel}>Min Params (B)</label>
            <Input
              type="number"
              className={styles.filterInput}
              value={minParams}
              onChange={(e) => setMinParams(e.target.value)}
              placeholder="e.g. 3"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className={styles.filterGroup}>
            <label className={styles.searchLabel}>Max Params (B)</label>
            <Input
              type="number"
              className={styles.filterInput}
              value={maxParams}
              onChange={(e) => setMaxParams(e.target.value)}
              placeholder="e.g. 13"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className={styles.filterGroup}>
            <label className={styles.searchLabel}>Sort By</label>
            <Row gap="xs" className={styles.sortWrapper}>
              <Select
                className={styles.sortSelect}
                value={sortBy}
                onChange={(e) => handleSortChange(e.target.value as HfSortField)}
              >
                {SORT_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <button
                className={styles.sortDirectionBtn}
                onClick={() => setSortAscending(!sortAscending)}
                title={sortAscending ? "Ascending" : "Descending"}
              >
                {sortAscending ? "↑" : "↓"}
              </button>
            </Row>
          </Stack>
        </Row>
      </Stack>

      {/* Results Section */}
      <Stack gap="base" className={styles.resultsSection}>
        {/* Error State */}
        {error && (
          <EmptyState
            icon={<span style={{ fontSize: '3rem' }}>⚠️</span>}
            title="Error"
            description={error}
          />
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
          <EmptyState
            icon="🔍"
            title="No models found"
            description="Try adjusting your search query or parameter filters."
          />
        )}

        {/* Results */}
        {!loading && models.length > 0 && (
          <Stack gap="base">
            <div className={styles.resultsHeader}>
              <span className={styles.resultsCount}>
                Showing {models.length} model{models.length !== 1 ? "s" : ""}
              </span>
            </div>

            <Stack gap="sm">
              {models.map((model) => (
                <ModelCard
                  key={model.id}
                  model={model}
                  onSelect={() => onSelectModel?.(model)}
                  isSelected={selectedModelId === model.id}
                />
              ))}
            </Stack>

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
          </Stack>
        )}
      </Stack>
    </Stack>
  );
};

export default HuggingFaceBrowser;
