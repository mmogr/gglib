import { FC } from "react";
import { ModelCardSkeleton } from './ModelCardSkeleton';
import { AlertTriangle, ArrowDown, ArrowUp, MessageSquare, Palette, Search } from "lucide-react";
import { HfModelKind, HfModelSummary, HfSortField } from "../../types";
import { ModelCard } from "./components/ModelCard";
import { useHuggingFaceSearch, SORT_OPTIONS } from "./hooks/useHuggingFaceSearch";
import { Button } from "../ui/Button";
import { Chip } from "../ui/Chip";
import { Icon } from "../ui/Icon";
import { IconButton } from '../ui/IconButton';
import { Input } from "../ui/Input";
import { Select } from "../ui/Select";
import { Stack, Row, EmptyState, Label } from "../primitives";
import { RecommendedModel } from '../ModelLibraryPanel/RecommendedModel';

interface HuggingFaceBrowserProps {
  /** Callback when a model is selected (clicked) for preview */
  onSelectModel?: (model: HfModelSummary | null) => void;
  /** Currently selected model ID (for highlighting) */
  selectedModelId?: string | null;
}

/** The two kinds a search looks for, as the toggle offers them. */
const KINDS: { id: HfModelKind; label: string; icon: typeof MessageSquare }[] = [
  { id: "chat", label: "Chat", icon: MessageSquare },
  { id: "image", label: "Image", icon: Palette },
];

/**
 * HuggingFace model browser component.
 * 
 * Allows searching, filtering, and browsing GGUF models from HuggingFace:
 * models that chat, or with the Image toggle models that draw.
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
    kind,
    setKind,

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

    // Actions
    handleSearchAction,
    handleLoadMore,
    handleKeyDown,
  } = useHuggingFaceSearch({ onSelectModel });

  return (
    <Stack gap="base" className="h-full overflow-hidden">
      {/* Search Section */}
      <Stack gap="sm" className="p-4 bg-surface border-b border-border-light">
        {/* Seeds the box rather than downloading: browsing results is still
            the user's decision, and the suggestion is advice, not an action. */}
        <RecommendedModel onUseRepo={setSearchQuery} />
        <Row gap="sm" align="end">
          <Stack gap="xs" className="flex-1">
            <Label size="xs" muted>Search models</Label>
            <Input
              type="text"
              size="lg"
              variant={searchError ? "error" : "default"}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search, paste user/repo, or user/repo:quant..."
            />
            {searchError && (
              <span className="block text-xs text-danger mt-[0.35rem]">{searchError}</span>
            )}
          </Stack>
          <Button
            className="shrink-0 whitespace-nowrap"
            onClick={handleSearchAction}
            disabled={loading}
            aria-label={buttonText}
          >
            {buttonText}
          </Button>
        </Row>

        <Row gap="base" className="mt-3" align="end" wrap>
          <Stack gap="xs">
            <Label size="xs" muted id="hf-kind-label">Kind</Label>
            <div role="group" aria-labelledby="hf-kind-label" className="flex items-center gap-xs h-8">
              {KINDS.map((choice) => (
                <Chip
                  key={choice.id}
                  selected={kind === choice.id}
                  onClick={() => setKind(choice.id)}
                  leftIcon={<Icon icon={choice.icon} size={12} />}
                >
                  {choice.label}
                </Chip>
              ))}
            </div>
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <Label size="xs" muted>Min params (B)</Label>
            <Input
              type="number"
              value={minParams}
              onChange={(e) => setMinParams(e.target.value)}
              placeholder="e.g. 3"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <Label size="xs" muted>Max params (B)</Label>
            <Input
              type="number"
              value={maxParams}
              onChange={(e) => setMaxParams(e.target.value)}
              placeholder="e.g. 13"
              min="0"
              step="0.1"
            />
          </Stack>
          <Stack gap="xs" className="flex-1 min-w-[120px] max-w-[180px]">
            <Label size="xs" muted>Sort by</Label>
            <Row gap="xs" className="min-w-0">
              <Select
                value={sortBy}
                onChange={(e) => handleSortChange(e.target.value as HfSortField)}
              >
                {SORT_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </Select>
              <IconButton
                label={sortAscending ? "Ascending" : "Descending"}
                variant="secondary"
                className="shrink-0"
                onClick={() => setSortAscending(!sortAscending)}
              >
                <Icon icon={sortAscending ? ArrowUp : ArrowDown} size={14} />
              </IconButton>
            </Row>
          </Stack>
        </Row>
      </Stack>

      {/* Results Section */}
      <Stack gap="base" className="flex-1 overflow-y-auto p-4">
        {/* Error State */}
        {error && (
          <EmptyState
            icon={<Icon icon={AlertTriangle} size={40} />}
            title="Error"
            description={error}
          />
        )}

        {/* Loading State */}
        {loading && (
          <Stack gap="sm" aria-label="Loading models">
            {Array.from({ length: 6 }, (_, i) => (
              <ModelCardSkeleton key={i} />
            ))}
          </Stack>
        )}

        {/* Empty State */}
        {!loading && !error && models.length === 0 && (
          <EmptyState
            icon={<Icon icon={Search} size={40} />}
            title="No models found"
            description="Try adjusting your search query or parameter filters."
          />
        )}

        {/* Results */}
        {!loading && models.length > 0 && (
          <Stack gap="base">
            <div className="flex items-center justify-between mb-4">
              <span className="text-sm text-text-secondary">
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
                <Button
                  variant="secondary"
                  size="lg"
                  onClick={handleLoadMore}
                  disabled={loadingMore}
                  isLoading={loadingMore}
                >
                  Load More
                </Button>
              </div>
            )}
          </Stack>
        )}
      </Stack>
    </Stack>
  );
};

export default HuggingFaceBrowser;
