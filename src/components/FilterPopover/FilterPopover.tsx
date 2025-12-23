import { FC, useRef } from 'react';
import { useClickOutside } from '../../hooks/useClickOutside';
import { RangeSlider } from '../RangeSlider';
import { ModelFilterOptions } from '../../types';
import { Button } from '../ui/Button';
import './FilterPopover.css';

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
    <div className="filter-popover" ref={popoverRef}>
      <div className="filter-popover-header">
        <span className="filter-popover-title">Filter Models</span>
        {hasActiveFilters && (
          <Button
            variant="ghost"
            size="sm"
            className="filter-clear-btn"
            onClick={onClearFilters}
            title="Clear all filters"
          >
            Clear
          </Button>
        )}
      </div>

      <div className="filter-popover-content">
        {/* Parameters Range Slider */}
        {hasParamRange && (
          <div className={`filter-section ${!paramRangeHasVariety ? 'filter-section-disabled' : ''}`}>
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
              <span className="filter-section-hint">All models have same size</span>
            )}
          </div>
        )}

        {/* Context Length Range Slider */}
        {hasContextRange && (
          <div className={`filter-section ${!contextRangeHasVariety ? 'filter-section-disabled' : ''}`}>
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
              <span className="filter-section-hint">All models have same context</span>
            )}
          </div>
        )}

        {/* Quantizations */}
        {hasQuantizations && (
          <div className={`filter-section ${!quantizationsHaveVariety ? 'filter-section-disabled' : ''}`}>
            <span className="filter-section-label">Quantization</span>
            {quantizationsHaveVariety ? (
              <div className="filter-checkbox-list">
                {filterOptions!.quantizations.map(quant => (
                  <label key={quant} className="filter-checkbox-item">
                    <input
                      type="checkbox"
                      checked={filters.selectedQuantizations.includes(quant)}
                      onChange={() => handleQuantizationToggle(quant)}
                      className="filter-checkbox"
                    />
                    <span className="filter-checkbox-label">{quant}</span>
                  </label>
                ))}
              </div>
            ) : (
              <span className="filter-section-hint">All models same quantization</span>
            )}
          </div>
        )}

        {/* Tags */}
        {hasTags && (
          <div className="filter-section">
            <span className="filter-section-label">Tags</span>
            <div className="filter-tag-list">
              {tags.map(tag => (
                <button
                  key={tag}
                  className={`filter-tag-chip ${filters.selectedTags.includes(tag) ? 'active' : ''}`}
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
          <div className="filter-empty-state">
            <span>No filter options available</span>
            <span className="filter-empty-hint">Add models to enable filtering</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default FilterPopover;
