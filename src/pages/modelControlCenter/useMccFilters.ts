import { useMemo } from 'react';
import type { ModelsInfo, ModelInfo } from '../../types/models';
import type { FilterState } from './modelControlCenterInterfaces';

/**
 * Custom hook to compute filtered models list based on applied filters.
 * Filters include:
 *   - Search query (name, architecture, repo_id)
 *   - Tags (AND logic: model must have all selected tags)
 *   - Parameter count range
 *   - Context length range
 *   - Quantizations
 * 
 * Returns the filtered models array.
 */
export function useMccFilters(
  allModels: ModelsInfo | null,
  searchQuery: string,
  filters: FilterState
): ModelInfo[] {
  return useMemo(() => {
    if (!allModels) return [];

    const modelsList = Object.values(allModels);

    return modelsList.filter((model) => {
      const matchesSearch =
        !searchQuery ||
        model.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        model.architecture?.toLowerCase().includes(searchQuery.toLowerCase()) ||
        model.hf_repo_id?.toLowerCase().includes(searchQuery.toLowerCase());

      const matchesTags =
        filters.selectedTags.length === 0 ||
        (model.tags && filters.selectedTags.every((tag) => model.tags!.includes(tag)));

      const matchesParams =
        filters.paramRange === null ||
        (model.param_count_b >= filters.paramRange[0] && model.param_count_b <= filters.paramRange[1]);

      const matchesContext =
        filters.contextRange === null ||
        (model.context_length >= filters.contextRange[0] &&
          model.context_length <= filters.contextRange[1]);

      const matchesQuants =
        filters.selectedQuantizations.length === 0 ||
        (model.available_quantizations &&
          filters.selectedQuantizations.some((q) =>
            model.available_quantizations!.includes(q)
          ));

      return matchesSearch && matchesTags && matchesParams && matchesContext && matchesQuants;
    });
  }, [allModels, searchQuery, filters]);
}
