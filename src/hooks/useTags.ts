import { useState, useEffect, useCallback } from 'react';
import {
  listTags,
  addModelTag,
  removeModelTag,
  getModelTags,
} from '../services/clients/tags';

export function useTags() {
  const [tags, setTags] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadTags = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const tagList = await listTags();
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
    await addModelTag(modelId, tag);
    await loadTags(); // Refresh tags list
  }, [loadTags]);

  const removeTagFromModel = useCallback(async (modelId: number, tag: string) => {
    await removeModelTag(modelId, tag);
    await loadTags(); // Refresh tags list
  }, [loadTags]);

  const fetchModelTags = useCallback(async (modelId: number): Promise<string[]> => {
    return await getModelTags(modelId);
  }, []);

  return {
    tags,
    loading,
    error,
    loadTags,
    addTagToModel,
    removeTagFromModel,
    getModelTags: fetchModelTags,
  };
}

