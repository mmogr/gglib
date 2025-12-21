import { useState, useEffect, useCallback } from 'react';

export interface ModelTagsState {
  modelTags: string[];
  newTag: string;
  setNewTag: (tag: string) => void;
  loadModelTags: () => Promise<void>;
  handleAddTag: () => Promise<void>;
  handleRemoveTag: (tag: string) => Promise<void>;
}

interface UseModelTagsConfig {
  modelId: number | undefined;
  getModelTags: (modelId: number) => Promise<string[]>;
  onAddTag: (modelId: number, tag: string) => Promise<void>;
  onRemoveTag: (modelId: number, tag: string) => Promise<void>;
  onRefresh?: () => Promise<void>;
}

/**
 * Hook for managing model tags state and CRUD operations.
 */
export function useModelTags({
  modelId,
  getModelTags,
  onAddTag,
  onRemoveTag,
  onRefresh,
}: UseModelTagsConfig): ModelTagsState {
  const [modelTags, setModelTags] = useState<string[]>([]);
  const [newTag, setNewTag] = useState('');

  const loadModelTags = useCallback(async () => {
    if (!modelId) return;
    try {
      const tags = await getModelTags(modelId);
      setModelTags(tags);
    } catch (error) {
      console.error('Failed to load model tags:', error);
    }
  }, [modelId, getModelTags]);

  // Load tags when model changes
  useEffect(() => {
    if (modelId) {
      loadModelTags();
    } else {
      setModelTags([]);
    }
  }, [modelId, loadModelTags]);

  const handleAddTag = useCallback(async () => {
    if (!modelId || !newTag.trim()) return;
    try {
      await onAddTag(modelId, newTag.trim());
      await loadModelTags();
      await onRefresh?.();
      setNewTag('');
    } catch (error) {
      console.error('Failed to add tag:', error);
    }
  }, [modelId, newTag, onAddTag, loadModelTags, onRefresh]);

  const handleRemoveTag = useCallback(async (tag: string) => {
    if (!modelId) return;
    try {
      await onRemoveTag(modelId, tag);
      await loadModelTags();
      await onRefresh?.();
    } catch (error) {
      console.error('Failed to remove tag:', error);
    }
  }, [modelId, onRemoveTag, loadModelTags, onRefresh]);

  return {
    modelTags,
    newTag,
    setNewTag,
    loadModelTags,
    handleAddTag,
    handleRemoveTag,
  };
}
