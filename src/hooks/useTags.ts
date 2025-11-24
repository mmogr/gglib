import { useState, useEffect, useCallback } from 'react';
import { TauriService } from '../services/tauri';

export function useTags() {
  const [tags, setTags] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadTags = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const tagList = await TauriService.listTags();
      setTags(tagList);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to load tags: ${errorMessage}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTags();
  }, [loadTags]);

  const addTagToModel = useCallback(async (modelId: number, tag: string) => {
    await TauriService.addModelTag(modelId, tag);
    await loadTags(); // Refresh tags list
  }, [loadTags]);

  const removeTagFromModel = useCallback(async (modelId: number, tag: string) => {
    await TauriService.removeModelTag(modelId, tag);
    await loadTags(); // Refresh tags list
  }, [loadTags]);

  const getModelTags = useCallback(async (modelId: number): Promise<string[]> => {
    return await TauriService.getModelTags(modelId);
  }, []);

  return {
    tags,
    loading,
    error,
    loadTags,
    addTagToModel,
    removeTagFromModel,
    getModelTags,
  };
}

