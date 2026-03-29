import { FC } from "react";
import { HfModelSummary, HfSortField } from "../../types";
import { ModelCard } from "./components/ModelCard";
import { useHuggingFaceSearch, SORT_OPTIONS } from "./hooks/useHuggingFaceSearch";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Stack, Row, EmptyState } from "../primitives";
import { cn } from '../../utils/cn';

/** Glass-effect form label */
const glassLabel = "block text-[0.8rem] font-medium text-text-secondary mb-[0.35rem] uppercase tracking-[0.03em]";
/** Glass-effect input override (small) */
const glassInput = "w-full px-3 py-2 bg-surface-elevated border border-border rounded-[6px] text-text text-[0.85rem] transition-all duration-200 ease-linear focus:outline-none focus:border-border-focus placeholder:text-text-muted";
/** Glass-effect input override (search box) */
const glassInputLg = "w-full px-[0.9rem] py-[0.6rem] bg-surface-elevated border border-border rounded-lg text-text text-[0.95rem] transition-all duration-200 ease-linear focus:outline-none focus:border-border-focus focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)] placeholder:text-text-muted";

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
    <Stack gap="base" className="h-full overflow-hidden">
      {/* Search Section */}
      <Stack gap="sm" className="p-4 bg-surface border-b border-border">
        <Row gap="sm" align="end">
          <Stack gap="xs" className="flex-1">
            <label className={glassLabel}>Search Models</label>
            <Input
              type="text"
              className={glassInputLg}
              variant={searchError ? "error" : "default"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search, paste user/repo, or user/repo:quant..."
            />
            {searchError && (
              <span className="block text-[0.75rem] text-danger mt-[0.35rem]">{searchError}</span>
            )}
          </Stack>
          <button
            className={cn(
              'px-[1.2rem] py-[0.6rem] border-none rounded-lg text-white font-semibold cursor-pointer transition-all duration-200 ease-linear whitespace-nowrap hover:not-disabled:-translate-y-px disabled:opacity-60 disabled:cursor-not-allowed',
              buttonVariant === 'accent' && 'bg-linear-to-br from-[#34d399] to-[#10b981] hover:not-disabled:shadow-[0_4px_12px_rgba(16,185,129,0.3)]',
              buttonVariant === 'primary' && 'bg-linear-to-br from-[#a78bfa] to-[#8b5cf6] hover:not-disabled:shadow-[0_4px_12px_rgba(139,92,246,0.3)]',
              buttonVariant !== 'accent' && buttonVariant !== 'primary' && 'bg-linear-to-br from-[#22d3ee] to-[#0ea5e9] hover:not-disabled:shadow-[0_4px_12px_rgba(34,211,238,0.3)]'
            )}
            onClick={handleSearchAction}
            disabled={loading}
            aria-label={buttonText}
          >
            {buttonText}
          </button>
        </Row>

        <Row gap="base" className="mt-3" align="end" wrap>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className={glassLabel}>Min Params (B)</label>
            <Input
              type="number"
              className={glassInput}
              value={minParams}
              onChange={(e) => setMinParams(e.target.value)}
              placeholder="e.g. 3"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className={glassLabel}>Max Params (B)</label>
            <Input
              type="number"
              className={glassInput}
              value={maxParams}
              onChange={(e) => setMaxParams(e.target.value)}
              placeholder="e.g. 13"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className={glassLabel}>Sort By</label>
            <Row gap="xs" className="min-w-0">
              <Select
                className="flex-1 min-w-0 px-3 py-2 bg-surface-elevated border border-border rounded-[6px] text-text text-[0.85rem] cursor-pointer transition-all duration-200 ease-linear appearance-none bg-[url('data:image/svg+xml,%3Csvg%20xmlns=\'http://www.w3.org/2000/svg\'%20width=\'12\'%20height=\'12\'%20viewBox=\'0%200%2024%2024\'%20fill=\'none\'%20stroke=\'%2394a3b8\'%20stroke-width=\'2\'%3E%3Cpath%20d=\'M6%209l6%206%206-6\'/%3E%3C/svg%3E')] bg-no-repeat bg-[right_0.5rem_center] pr-7 focus:outline-none focus:border-border-focus"
                value={sortBy}
                onChange={(e) => handleSortChange(e.target.value as HfSortField)}
              >
                {SORT_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value} className="bg-surface text-text">
                    {option.label}
                  </option>
                ))}
              </Select>
              <button
                className="shrink-0 px-[0.65rem] py-2 bg-surface-elevated border border-border rounded-[6px] text-text-secondary text-[0.9rem] cursor-pointer transition-all duration-150 ease-linear leading-none hover:bg-surface-hover hover:text-text hover:border-border-hover"
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
      <Stack gap="base" className="flex-1 overflow-y-auto p-4">
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
          <div className="flex flex-col items-center justify-center p-12 text-center text-text-muted">
            <div className="w-8 h-8 border-3 border-border border-t-primary-light rounded-full animate-spin mb-4"></div>
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
            <div className="flex items-center justify-between mb-4">
              <span className="text-[0.85rem] text-text-secondary">
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
              <div className="p-4 flex justify-center">
                <button
                  className="px-6 py-[0.6rem] bg-surface-elevated border border-border rounded-lg text-text font-medium cursor-pointer transition-all duration-200 ease-linear hover:not-disabled:bg-surface-hover hover:not-disabled:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
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
