import { FC, useRef } from 'react';
import { useClickOutside } from '../../hooks/useClickOutside';
import { RangeSlider } from '../RangeSlider';
import { ModelFilterOptions } from '../../types';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

export interface FilterState {
  paramRange: [number, number] | null;
  contextRange: [number, number] | null;
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
  /** Callback to clear all filters */
  onClearFilters: () => void;
}

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
 * Filter popover component for the model library.
 * Contains range sliders for params/context and checkboxes for quantizations/tags.
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
  const hasQuantizations = filterOptions?.quantizations && filterOptions.quantizations.length > 0;
  const hasTags = tags.length > 0;

  // Check if param range has more than one unique value
  const paramRangeHasVariety = hasParamRange && 
    filterOptions!.param_range!.min !== filterOptions!.param_range!.max;
  
  // Check if context range has more than one unique value
  const contextRangeHasVariety = hasContextRange && 
    filterOptions!.context_range!.min !== filterOptions!.context_range!.max;

  // Check if quantizations has more than one option
  const quantizationsHaveVariety = hasQuantizations && 
    filterOptions!.quantizations.length > 1;

  // Check if there are any active filters
  const hasActiveFilters = 
    filters.paramRange !== null ||
    filters.contextRange !== null ||
    filters.selectedQuantizations.length > 0 ||
    filters.selectedTags.length > 0;

  const handleParamRangeChange = (min: number, max: number) => {
    // If back to full range, set to null
    const fullRange = filterOptions?.param_range;
    if (fullRange && min <= fullRange.min && max >= fullRange.max) {
      onFiltersChange({ ...filters, paramRange: null });
    } else {
      onFiltersChange({ ...filters, paramRange: [min, max] });
    }
  };

  const handleContextRangeChange = (min: number, max: number) => {
    // If back to full range, set to null
    const fullRange = filterOptions?.context_range;
    if (fullRange && min <= fullRange.min && max >= fullRange.max) {
      onFiltersChange({ ...filters, contextRange: null });
    } else {
      onFiltersChange({ ...filters, contextRange: [min, max] });
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

  // Get current values for sliders (use filter state or fall back to full range)
  const currentParamMin = filters.paramRange?.[0] ?? filterOptions?.param_range?.min ?? 0;
  const currentParamMax = filters.paramRange?.[1] ?? filterOptions?.param_range?.max ?? 100;
  const currentContextMin = filters.contextRange?.[0] ?? filterOptions?.context_range?.min ?? 0;
  const currentContextMax = filters.contextRange?.[1] ?? filterOptions?.context_range?.max ?? 128000;

  return (
    <div className="absolute top-full right-0 mt-xs bg-surface border border-border rounded-md shadow-[0_4px_16px_rgba(0,0,0,0.3)] min-w-[280px] max-w-[320px] z-[1000] overflow-hidden" ref={popoverRef}>
      <div className="flex items-center justify-between py-sm px-md border-b border-border bg-surface-elevated">
        <span className="text-sm font-semibold text-text">Filter Models</span>
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            className="py-[4px] px-[8px] text-xs font-medium text-primary border border-primary rounded-sm hover:bg-primary hover:text-white"
            onClick={onClearFilters}
            title="Clear all filters"
          >
            Clear
          </Button>
        )}
      </div>

      <div className="max-h-[400px] overflow-y-auto py-sm px-md scrollbar-thin">
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

        {/* Quantizations */}
        {hasQuantizations && (
          <div className={cn("py-sm border-b border-border last:border-b-0", !quantizationsHaveVariety && "opacity-50")}>
            <span className="block text-sm font-medium text-text mb-xs">Quantization</span>
            {quantizationsHaveVariety ? (
              <div className="flex flex-col gap-xs mt-xs">
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
              </div>
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

        {/* Empty state */}
        {!hasParamRange && !hasContextRange && !hasQuantizations && !hasTags && (
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
