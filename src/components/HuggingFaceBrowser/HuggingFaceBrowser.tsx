import { FC } from "react";
import { HfModelSummary, HfSortField } from "../../types";
import { ModelCard } from "./components/ModelCard";
import { useHuggingFaceSearch, SORT_OPTIONS } from "./hooks/useHuggingFaceSearch";
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Stack, Row, EmptyState } from "../primitives";
import { cn } from '../../utils/cn';

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
    <Stack gap="base" className="flex flex-col h-full overflow-hidden">
      {/* Search Section */}
      <Stack gap="sm" className="p-4 bg-[rgba(255,255,255,0.02)] border-b border-[rgba(255,255,255,0.08)]">
        <Row gap="sm" className="flex gap-3 items-end" align="end">
          <Stack gap="xs" className="flex-1">
            <label className="block text-[0.8rem] font-medium text-text-secondary mb-[0.35rem] uppercase tracking-[0.03em]">Search Models</label>
            <Input
              type="text"
              className="w-full px-[0.9rem] py-[0.6rem] bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-lg text-text text-[0.95rem] transition-all duration-200 ease-linear focus:outline-none focus:border-[rgba(34,211,238,0.5)] focus:bg-[rgba(255,255,255,0.07)] focus:shadow-[0_0_0_3px_rgba(34,211,238,0.1)] placeholder:text-text-muted"
              variant={searchError ? "error" : "default"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search, paste user/repo, or user/repo:quant..."
            />
            {searchError && (
              <span className="block text-[0.75rem] text-[#ef4444] mt-[0.35rem]">{searchError}</span>
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

        <Row gap="base" className="flex gap-3 mt-3 items-end flex-wrap" wrap>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className="block text-[0.8rem] font-medium text-text-secondary mb-[0.35rem] uppercase tracking-[0.03em]">Min Params (B)</label>
            <Input
              type="number"
              className="w-full px-3 py-2 bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-[6px] text-text text-[0.85rem] transition-all duration-200 ease-linear focus:outline-none focus:border-[rgba(34,211,238,0.5)] focus:bg-[rgba(255,255,255,0.07)] placeholder:text-[#64748b]"
              value={minParams}
              onChange={(e) => setMinParams(e.target.value)}
              placeholder="e.g. 3"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className="block text-[0.8rem] font-medium text-text-secondary mb-[0.35rem] uppercase tracking-[0.03em]">Max Params (B)</label>
            <Input
              type="number"
              className="w-full px-3 py-2 bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-[6px] text-text text-[0.85rem] transition-all duration-200 ease-linear focus:outline-none focus:border-[rgba(34,211,238,0.5)] focus:bg-[rgba(255,255,255,0.07)] placeholder:text-[#64748b]"
              value={maxParams}
              onChange={(e) => setMaxParams(e.target.value)}
              placeholder="e.g. 13"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <label className="block text-[0.8rem] font-medium text-text-secondary mb-[0.35rem] uppercase tracking-[0.03em]">Sort By</label>
            <Row gap="xs" className="flex gap-1 min-w-0">
              <Select
                className="flex-1 min-w-0 px-3 py-2 bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-[6px] text-[#f1f5f9] text-[0.85rem] cursor-pointer transition-all duration-200 ease-linear appearance-none bg-[url('data:image/svg+xml,%3Csvg%20xmlns=\x27http://www.w3.org/2000/svg\x27%20width=\x2712\x27%20height=\x2712\x27%20viewBox=\x270%200%2024%2024\x27%20fill=\x27none\x27%20stroke=\x27%2394a3b8\x27%20stroke-width=\x272\x27%3E%3Cpath%20d=\x27M6%209l6%206%206-6\x27/%3E%3C/svg%3E')] bg-no-repeat bg-[right_0.5rem_center] pr-7 focus:outline-none focus:border-[rgba(34,211,238,0.5)] focus:bg-[rgba(255,255,255,0.07)]"
                value={sortBy}
                onChange={(e) => handleSortChange(e.target.value as HfSortField)}
              >
                {SORT_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value} className="bg-[#1e293b] text-[#f1f5f9]">
                    {option.label}
                  </option>
                ))}
              </Select>
              <button
                className="shrink-0 px-[0.65rem] py-2 bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-[6px] text-[#94a3b8] text-[0.9rem] cursor-pointer transition-all duration-150 ease-linear leading-none hover:bg-[rgba(255,255,255,0.08)] hover:text-[#f1f5f9] hover:border-[rgba(255,255,255,0.15)]"
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
          <div className="flex flex-col items-center justify-center p-12 text-center text-[#64748b]">
            <div className="w-8 h-8 border-3 border-[rgba(255,255,255,0.1)] border-t-[#22d3ee] rounded-full animate-spin mb-4"></div>
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
              <span className="text-[0.85rem] text-[#94a3b8]">
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
                  className="px-6 py-[0.6rem] bg-[rgba(255,255,255,0.05)] border border-[rgba(255,255,255,0.1)] rounded-lg text-[#e2e8f0] font-medium cursor-pointer transition-all duration-200 ease-linear hover:not-disabled:bg-[rgba(255,255,255,0.08)] hover:not-disabled:border-[rgba(255,255,255,0.15)] disabled:opacity-50 disabled:cursor-not-allowed"
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
