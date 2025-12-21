import { useCallback, useMemo, useState } from 'react';
import type { FilterState } from '../../components/FilterPopover';
import type { AddDownloadSubTab } from '../../components/ModelLibraryPanel/AddDownloadContent';
import type { GgufModel } from '../../types';

type RefreshDeps = {
  loadModels: () => Promise<void>;
  refreshFilterOptions: () => Promise<void>;
  loadTags: () => Promise<void>;
};

interface UseMccFiltersArgs extends RefreshDeps {
  models: GgufModel[];
  addModel: (filePath: string) => Promise<void>;
}

export interface UseMccFiltersResult {
  searchQuery: string;
  setSearchQuery: (value: string) => void;
  filters: FilterState;
  onFiltersChange: (filters: FilterState) => void;
  onClearFilters: () => void;
  filteredModels: GgufModel[];
  activeSubTab: AddDownloadSubTab;
  setActiveSubTab: (tab: AddDownloadSubTab) => void;
  handleModelAdded: (filePath?: string) => Promise<void>;
}

export function useMccFilters({
  models,
  addModel,
  loadModels,
  refreshFilterOptions,
  loadTags,
}: UseMccFiltersArgs): UseMccFiltersResult {
  const [searchQuery, setSearchQuery] = useState('');
  const [activeSubTab, setActiveSubTab] = useState<AddDownloadSubTab>('browse');
  const [filters, setFilters] = useState<FilterState>({
    paramRange: null,
    contextRange: null,
    selectedQuantizations: [],
    selectedTags: [],
  });

  const onFiltersChange = useCallback((newFilters: FilterState) => {
    setFilters(newFilters);
  }, []);

  const onClearFilters = useCallback(() => {
    setFilters({
      paramRange: null,
      contextRange: null,
      selectedQuantizations: [],
      selectedTags: [],
    });
  }, []);

  const filteredModels = useMemo(() => {
    return models.filter((model) => {
      const matchesSearch =
        !searchQuery ||
        model.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        model.architecture?.toLowerCase().includes(searchQuery.toLowerCase()) ||
        model.hf_repo_id?.toLowerCase().includes(searchQuery.toLowerCase());

      const matchesTags =
        filters.selectedTags.length === 0 ||
        (model.tags && filters.selectedTags.some((tag) => model.tags!.includes(tag)));

      const matchesParams =
        filters.paramRange === null ||
        (model.param_count_b >= filters.paramRange[0] && model.param_count_b <= filters.paramRange[1]);

      const matchesContext =
        filters.contextRange === null ||
        model.context_length === undefined ||
        model.context_length === null ||
        (model.context_length >= filters.contextRange[0] && model.context_length <= filters.contextRange[1]);

      const matchesQuantization =
        filters.selectedQuantizations.length === 0 ||
        (model.quantization && filters.selectedQuantizations.includes(model.quantization));

      return matchesSearch && matchesTags && matchesParams && matchesContext && matchesQuantization;
    });
  }, [models, searchQuery, filters]);

  const refreshFilterDeps = useCallback(async () => {
    await Promise.all([loadModels(), refreshFilterOptions(), loadTags()]);
  }, [loadModels, refreshFilterOptions, loadTags]);

  const handleModelAdded = useCallback(
    async (filePath?: string) => {
      if (filePath) {
        await addModel(filePath);
      }
      await refreshFilterDeps();
    },
    [addModel, refreshFilterDeps]
  );

  return {
    searchQuery,
    setSearchQuery,
    filters,
    onFiltersChange,
    onClearFilters,
    filteredModels,
    activeSubTab,
    setActiveSubTab,
    handleModelAdded,
  };
}
