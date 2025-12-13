import { useState, useEffect, useCallback } from 'react';
import { getModelFilterOptions } from '../services/clients/models';
import { ModelFilterOptions } from '../types';

/**
 * Hook to fetch model filter options (quantizations, param range, context range)
 * for the model library filter UI.
 */
export function useModelFilterOptions() {
  const [filterOptions, setFilterOptions] = useState<ModelFilterOptions | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadFilterOptions = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const options = await getModelFilterOptions();
      setFilterOptions(options);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to load filter options: ${errorMessage}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadFilterOptions();
  }, [loadFilterOptions]);

  return {
    filterOptions,
    loading,
    error,
    refresh: loadFilterOptions,
  };
}
