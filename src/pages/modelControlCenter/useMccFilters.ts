import { useCallback, useEffect, useMemo, useState } from 'react';
import type { FilterState } from '../../components/FilterPopover';
import type { AddDownloadSubTab } from '../../components/ModelLibraryPanel/AddDownloadContent';
import type { GgufModel } from '../../types';
import { getTransport } from '../../services/transport';
import type { ModelListQuery } from '../../services/transport/api/models/local';

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

/**
 * The daemon's list query for a FilterState: every filter that is set, and
 * none that is not. A range the popover has not narrowed is `null` there and
 * left out here, and so is an empty selection.
 */
function modelListQuery(filters: FilterState): ModelListQuery {
  const [min_params, max_params] = filters.paramRange ?? [];
  const [min_context, max_context] = filters.contextRange ?? [];
  const [min_speed, max_speed] = filters.speedRange ?? [];
  return {
    sort: filters.sortBy,
    order: filters.sortOrder,
    min_params,
    max_params,
    min_context,
    max_context,
    min_speed,
    max_speed,
    quantizations: filters.selectedQuantizations.join(',') || undefined,
    tags: filters.selectedTags.join(',') || undefined,
  };
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

  // Server-fetched model list.  Initialised from the parent's `models` prop so
  // the first render shows data immediately (no empty flash while the first
  // debounced request is in-flight).
  const [serverModels, setServerModels] = useState<GgufModel[]>(models);

  const [filters, setFilters] = useState<FilterState>({
    sortBy: 'added_at',
    sortOrder: 'desc',
    paramRange: null,
    contextRange: null,
    speedRange: null,
    selectedQuantizations: [],
    selectedTags: [],
  });

  const onFiltersChange = useCallback((newFilters: FilterState) => {
    setFilters(newFilters);
  }, []);

  // Clear filters but preserve the sort preference.
  const onClearFilters = useCallback(() => {
    setFilters(prev => ({
      sortBy: prev.sortBy,
      sortOrder: prev.sortOrder,
      paramRange: null,
      contextRange: null,
      speedRange: null,
      selectedQuantizations: [],
      selectedTags: [],
    }));
  }, []);

  // Debounced server-side fetch.  Runs 300 ms after the last filter change.
  // Both the timer and any in-flight request are cancelled on cleanup so stale
  // results never overwrite a newer response.
  useEffect(() => {
    let isCurrent = true;
    const timer = setTimeout(async () => {
      try {
        const data = await getTransport().listModels(modelListQuery(filters));
        if (isCurrent) setServerModels(data);
      } catch {
        // Network error or backend down — keep the current list visible.
      }
    }, 300);
    return () => {
      isCurrent = false;
      clearTimeout(timer);
    };
  }, [filters]);

  // Client-side text search applied on top of the server-filtered list.
  // The backend has no full-text search endpoint; a lightweight filter here
  // avoids an extra round-trip for every keystroke.
  const filteredModels = useMemo(() => {
    if (!searchQuery) return serverModels;
    const q = searchQuery.toLowerCase();
    return serverModels.filter(
      (model) =>
        model.name.toLowerCase().includes(q) ||
        model.architecture?.toLowerCase().includes(q) ||
        model.hfRepoId?.toLowerCase().includes(q),
    );
  }, [serverModels, searchQuery]);

  const refreshFilterDeps = useCallback(async () => {
    await Promise.all([loadModels(), refreshFilterOptions(), loadTags()]);
  }, [loadModels, refreshFilterOptions, loadTags]);

  // After a mutation (add/remove) we refresh shared state then immediately
  // re-fetch the filtered list so the new model appears without the 300 ms lag.
  const handleModelAdded = useCallback(
    async (filePath?: string) => {
      if (filePath) {
        await addModel(filePath);
      }
      await refreshFilterDeps();
      try {
        setServerModels(await getTransport().listModels(modelListQuery(filters)));
      } catch {
        // Ignore; the debounced effect will retry shortly.
      }
    },
    [addModel, refreshFilterDeps, filters],
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
