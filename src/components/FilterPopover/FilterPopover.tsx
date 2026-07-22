import { FC, useRef } from 'react';
import { ArrowDown, ArrowUp } from 'lucide-react';
import { useClickOutside } from '../../hooks/useClickOutside';
import { RangeSlider } from '../RangeSlider';
import type { ModelFilterOptions, ModelSortBy, SortOrder } from '../../types';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Stack } from '../primitives';
import { cn } from '../../utils/cn';

export interface FilterState {
  sortBy: ModelSortBy;
  sortOrder: SortOrder;
  paramRange: [number, number] | null;
  contextRange: [number, number] | null;
  speedRange: [number, number] | null;
  selectedQuantizations: string[];
  selectedTags: string[];
}

interface FilterPopoverProps {
  isOpen: boolean;
  onClose: () => void;
  /** Available filter options from the database */
  filterOptions: ModelFilterOptions | null;
  /** Available tags (from useTags hook) */
  tags: string[];
  /** Current filter state */
  filters: FilterState;
  /** Callback when filters change */
  onFiltersChange: (filters: FilterState) => void;
  /** Callback to clear all filters (sort preference is preserved) */
  onClearFilters: () => void;
}

const SORT_OPTIONS: [ModelSortBy, string][] = [
  ['added_at', 'Added'],
  ['name', 'Name'],
  ['param_count', 'Params'],
  ['latest_tg_tps', 'Speed'],
];

/**
 * Format parameter count for display (e.g., "7.0B", "70B", "0.5B")
 */
const formatParamCount = (value: number): string => {
  if (value >= 1) {
    return `${value.toFixed(1)}B`;
  }
  return `${(value * 1000).toFixed(0)}M`;
};

/**
 * Format context length for display (e.g., "4K", "32K", "128K")
 */
const formatContextLength = (value: number): string => {
  if (value >= 1000) {
    return `${Math.round(value / 1000)}K`;
  }
  return value.toString();
};

/**
 * Format token-generation throughput for display (e.g., "12.3 t/s")
 */
const formatSpeed = (value: number): string => `${value.toFixed(1)} t/s`;

/**
 * Filter + sort popover for the model library.
 * Contains a sort section (field + direction) at the top, followed by range
 * sliders for params/context/speed and checkboxes for quantizations/tags.
 * Clearing filters resets all filter controls but preserves the sort preference.
 */
const FilterPopover: FC<FilterPopoverProps> = ({
  isOpen,
  onClose,
  filterOptions,
  tags,
  filters,
  onFiltersChange,
  onClearFilters,
}) => {
  const popoverRef = useRef<HTMLDivElement>(null);
  useClickOutside(popoverRef, onClose, isOpen);

  if (!isOpen) return null;

  const hasParamRange = filterOptions?.param_range != null;
  const hasContextRange = filterOptions?.context_range != null;
  const hasSpeedRange = filterOptions?.speed_range != null;
  const hasQuantizations = filterOptions?.quantizations && filterOptions.quantizations.length > 0;
  const hasTags = tags.length > 0;

  const paramRangeHasVariety = hasParamRange &&
    filterOptions!.param_range!.min !== filterOptions!.param_range!.max;

  const contextRangeHasVariety = hasContextRange &&
    filterOptions!.context_range!.min !== filterOptions!.context_range!.max;

  const speedRangeHasVariety = hasSpeedRange &&
    filterOptions!.speed_range!.min !== filterOptions!.speed_range!.max;

  const quantizationsHaveVariety = hasQuantizations &&
    filterOptions!.quantizations.length > 1;

  // Active-filter check intentionally excludes sortBy / sortOrder so that Clear
  // only appears when actual filter constraints are in place.
  const hasActiveFilters =
    filters.paramRange !== null ||
    filters.contextRange !== null ||
    filters.speedRange !== null ||
    filters.selectedQuantizations.length > 0 ||
    filters.selectedTags.length > 0;

  const handleParamRangeChange = (min: number, max: number) => {
    const fullRange = filterOptions?.param_range;
    if (fullRange && min <= fullRange.min && max >= fullRange.max) {
      onFiltersChange({ ...filters, paramRange: null });
    } else {
      onFiltersChange({ ...filters, paramRange: [min, max] });
    }
  };

  const handleContextRangeChange = (min: number, max: number) => {
    const fullRange = filterOptions?.context_range;
    if (fullRange && min <= fullRange.min && max >= fullRange.max) {
      onFiltersChange({ ...filters, contextRange: null });
    } else {
      onFiltersChange({ ...filters, contextRange: [min, max] });
    }
  };

  const handleSpeedRangeChange = (min: number, max: number) => {
    const fullRange = filterOptions?.speed_range;
    if (fullRange && min <= fullRange.min && max >= fullRange.max) {
      onFiltersChange({ ...filters, speedRange: null });
    } else {
      onFiltersChange({ ...filters, speedRange: [min, max] });
    }
  };

  const handleQuantizationToggle = (quant: string) => {
    const selected = filters.selectedQuantizations;
    if (selected.includes(quant)) {
      onFiltersChange({
        ...filters,
        selectedQuantizations: selected.filter(q => q !== quant),
      });
    } else {
      onFiltersChange({
        ...filters,
        selectedQuantizations: [...selected, quant],
      });
    }
  };

  const handleTagToggle = (tag: string) => {
    const selected = filters.selectedTags;
    if (selected.includes(tag)) {
      onFiltersChange({
        ...filters,
        selectedTags: selected.filter(t => t !== tag),
      });
    } else {
      onFiltersChange({
        ...filters,
        selectedTags: [...selected, tag],
      });
    }
  };

  const currentParamMin = filters.paramRange?.[0] ?? filterOptions?.param_range?.min ?? 0;
  const currentParamMax = filters.paramRange?.[1] ?? filterOptions?.param_range?.max ?? 100;
  const currentContextMin = filters.contextRange?.[0] ?? filterOptions?.context_range?.min ?? 0;
  const currentContextMax = filters.contextRange?.[1] ?? filterOptions?.context_range?.max ?? 128000;
  const currentSpeedMin = filters.speedRange?.[0] ?? filterOptions?.speed_range?.min ?? 0;
  const currentSpeedMax = filters.speedRange?.[1] ?? filterOptions?.speed_range?.max ?? 200;

  return (
    <div className="absolute top-full right-0 mt-xs bg-surface border border-border rounded-md shadow-[0_4px_16px_rgba(0,0,0,0.3)] min-w-[280px] max-w-[320px] z-[1000] overflow-hidden" ref={popoverRef}>
      <div className="flex items-center justify-between py-sm px-md border-b border-border bg-surface-elevated">
        <span className="text-sm font-semibold text-text">Sort & Filter</span>
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            className="py-[4px] px-[8px] text-xs font-medium text-primary border border-primary rounded-sm hover:bg-primary hover:text-white"
            onClick={onClearFilters}
            title="Clear all filters (sort preference is kept)"
          >
            Clear
          </Button>
        )}
      </div>

      <div className="max-h-[440px] overflow-y-auto py-sm px-md scrollbar-thin">
        {/* Sort Section */}
        <div className="py-sm border-b border-border">
          <span className="block text-sm font-medium text-text mb-xs">Sort By</span>
          <div className="flex flex-wrap gap-xs mt-xs">
            {SORT_OPTIONS.map(([value, label]) => (
              <button
                key={value}
                className={cn(
                  "py-[4px] px-[10px] text-xs font-medium rounded-sm border cursor-pointer transition-all duration-150",
                  filters.sortBy === value
                    ? "bg-primary border-primary text-white"
                    : "text-text-secondary bg-surface-elevated border-border hover:border-primary hover:text-text"
                )}
                onClick={() => onFiltersChange({ ...filters, sortBy: value })}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="flex gap-xs mt-xs">
            {(['desc', 'asc'] as SortOrder[]).map(dir => (
              <button
                key={dir}
                className={cn(
                  "flex-1 inline-flex items-center justify-center gap-xs py-[4px] text-xs font-medium rounded-sm border cursor-pointer transition-all duration-150",
                  filters.sortOrder === dir
                    ? "bg-surface-elevated border-primary text-text"
                    : "bg-surface border-border text-text-muted hover:border-primary hover:text-text"
                )}
                onClick={() => onFiltersChange({ ...filters, sortOrder: dir })}
              >
                <Icon icon={dir === 'desc' ? ArrowDown : ArrowUp} size={12} />
                {dir === 'desc' ? 'Desc' : 'Asc'}
              </button>
            ))}
          </div>
        </div>

        {/* Parameters Range Slider */}
        {hasParamRange && (
          <div className={cn("py-sm border-b border-border last:border-b-0", !paramRangeHasVariety && "opacity-50")}>
            <RangeSlider
              label="Parameters"
              min={filterOptions!.param_range!.min}
              max={filterOptions!.param_range!.max}
              minValue={currentParamMin}
              maxValue={currentParamMax}
              step={0.1}
              onChange={handleParamRangeChange}
              formatValue={formatParamCount}
              disabled={!paramRangeHasVariety}
            />
            {!paramRangeHasVariety && (
              <span className="block text-xs text-text-muted italic mt-xs">All models have same size</span>
            )}
          </div>
        )}

        {/* Context Length Range Slider */}
        {hasContextRange && (
          <div className={cn("py-sm border-b border-border last:border-b-0", !contextRangeHasVariety && "opacity-50")}>
            <RangeSlider
              label="Context Length"
              min={filterOptions!.context_range!.min}
              max={filterOptions!.context_range!.max}
              minValue={currentContextMin}
              maxValue={currentContextMax}
              step={1024}
              onChange={handleContextRangeChange}
              formatValue={formatContextLength}
              disabled={!contextRangeHasVariety}
            />
            {!contextRangeHasVariety && (
              <span className="block text-xs text-text-muted italic mt-xs">All models have same context</span>
            )}
          </div>
        )}

        {/* Speed Range Slider — only shown when benchmark data is available */}
        {hasSpeedRange && (
          <div className={cn("py-sm border-b border-border last:border-b-0", !speedRangeHasVariety && "opacity-50")}>
            <RangeSlider
              label="Speed (t/s)"
              min={filterOptions!.speed_range!.min}
              max={filterOptions!.speed_range!.max}
              minValue={currentSpeedMin}
              maxValue={currentSpeedMax}
              step={0.1}
              onChange={handleSpeedRangeChange}
              formatValue={formatSpeed}
              disabled={!speedRangeHasVariety}
            />
            {!speedRangeHasVariety && (
              <span className="block text-xs text-text-muted italic mt-xs">All benchmarks same speed</span>
            )}
          </div>
        )}

        {/* Quantizations */}
        {hasQuantizations && (
          <div className={cn("py-sm border-b border-border last:border-b-0", !quantizationsHaveVariety && "opacity-50")}>
            <span className="block text-sm font-medium text-text mb-xs">Quantization</span>
            {quantizationsHaveVariety ? (
              <Stack gap="xs" className="mt-xs">
                {filterOptions!.quantizations.map(quant => (
                  <label key={quant} className="flex items-center gap-sm cursor-pointer py-[4px] hover:bg-surface-elevated hover:rounded-sm hover:mx-[-4px] hover:px-[4px]">
                    <input
                      type="checkbox"
                      checked={filters.selectedQuantizations.includes(quant)}
                      onChange={() => handleQuantizationToggle(quant)}
                      className="w-[16px] h-[16px] accent-primary cursor-pointer"
                    />
                    <span className="text-sm text-text">{quant}</span>
                  </label>
                ))}
              </Stack>
            ) : (
              <span className="block text-xs text-text-muted italic mt-xs">All models same quantization</span>
            )}
          </div>
        )}

        {/* Tags */}
        {hasTags && (
          <div className="py-sm border-b border-border last:border-b-0">
            <span className="block text-sm font-medium text-text mb-xs">Tags</span>
            <div className="flex flex-wrap gap-xs mt-xs">
              {tags.map(tag => (
                <button
                  key={tag}
                  className={cn(
                    "py-[4px] px-[10px] text-xs font-medium text-text-secondary bg-surface-elevated border border-border rounded-lg cursor-pointer transition-all duration-150 hover:border-primary hover:text-text",
                    filters.selectedTags.includes(tag) && "bg-primary border-primary text-white"
                  )}
                  onClick={() => handleTagToggle(tag)}
                >
                  {tag}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Empty state — sort section is always visible so this only fires when truly nothing else is available */}
        {!hasParamRange && !hasContextRange && !hasSpeedRange && !hasQuantizations && !hasTags && (
          <div className="flex flex-col items-center justify-center p-lg text-center">
            <span className="text-sm text-text-secondary">No filter options available</span>
            <span className="text-xs text-text-muted mt-xs">Add models to enable filtering</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default FilterPopover;
