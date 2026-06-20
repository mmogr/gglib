import { useState, useEffect, useCallback } from 'react';
import type { ModelDetail } from '../../../types';
import { appLogger } from '../../../services/platform';

export interface ModelDetailState {
  modelDetail: ModelDetail | null;
  tags: string[];
  newTagInput: string;
  setNewTagInput: (tag: string) => void;
  reload: () => Promise<void>;
  addTag: () => Promise<void>;
  removeTag: (tag: string) => Promise<void>;
  isLoading: boolean;
}

interface UseModelDetailConfig {
  modelId: number | undefined;
  getModelDetail: (modelId: number) => Promise<ModelDetail | null>;
  onAddTag: (modelId: number, tag: string) => Promise<void>;
  onRemoveTag: (modelId: number, tag: string) => Promise<void>;
  onRefresh?: () => Promise<void>;
}

/**
 * Hook for fetching full model detail and managing tag mutations.
 *
 * Replaces the two-call pattern (getModel + getModelTags) with a single
 * call to GET /api/models/:id/detail. Tags are derived from the detail
 * response and stay in sync after mutations.
 */
export function useModelDetail({
  modelId,
  getModelDetail,
  onAddTag,
  onRemoveTag,
  onRefresh,
}: UseModelDetailConfig): ModelDetailState {
  const [modelDetail, setModelDetail] = useState<ModelDetail | null>(null);
  const [newTagInput, setNewTagInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);

  const reload = useCallback(async () => {
    if (!modelId) return;
    setIsLoading(true);
    try {
      const detail = await getModelDetail(modelId);
      setModelDetail(detail);
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to load model detail', { error, modelId });
    } finally {
      setIsLoading(false);
    }
  }, [modelId, getModelDetail]);

  // Fetch detail whenever the selected model changes
  useEffect(() => {
    if (modelId) {
      reload();
    } else {
      setModelDetail(null);
    }
  }, [modelId, reload]);

  const addTag = useCallback(async () => {
    if (!modelId || !newTagInput.trim()) return;
    try {
      await onAddTag(modelId, newTagInput.trim());
      await reload();
      await onRefresh?.();
      setNewTagInput('');
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to add tag', { error, modelId, tag: newTagInput });
    }
  }, [modelId, newTagInput, onAddTag, reload, onRefresh]);

  const removeTag = useCallback(async (tag: string) => {
    if (!modelId) return;
    try {
      await onRemoveTag(modelId, tag);
      await reload();
      await onRefresh?.();
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to remove tag', { error, modelId, tag });
    }
  }, [modelId, onRemoveTag, reload, onRefresh]);

  return {
    modelDetail,
    tags: modelDetail?.tags ?? [],
    newTagInput,
    setNewTagInput,
    reload,
    addTag,
    removeTag,
    isLoading,
  };
}
