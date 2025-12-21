import { useState, useEffect, useCallback } from 'react';
import { GgufModel } from '../types';
import {
  listModels,
  addModel as addModelService,
  removeModel as removeModelService,
  updateModel as updateModelService,
} from '../services/clients/models';
// TRANSPORT_EXCEPTION: setSelectedModel is desktop-only (menu sync)
import { setSelectedModel } from '../services/platform';

export function useModels() {
  const [models, setModels] = useState<GgufModel[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadModels = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const modelList = await listModels();
      setModels(modelList);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to load models: ${errorMessage}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadModels();
  }, [loadModels]);

  // Sync selected model with native menu state (Tauri only)
  const selectModel = useCallback((id: number | null) => {
    setSelectedModelId(id);
    // Sync with backend for menu state updates (no-op in web mode)
    setSelectedModel(id).catch((err) => {
      console.warn('Failed to sync model selection with menu:', err);
    });
  }, []);

  const selectedModel = models.find(m => m.id === selectedModelId) || null;

  const addModel = useCallback(async (filePath: string) => {
    await addModelService({ filePath });
    await loadModels();
  }, [loadModels]);

  const removeModel = useCallback(async (id: number, _force: boolean = false) => {
    // Note: 'force' param not supported by Transport - caller should handle confirmation
    await removeModelService(id);
    if (selectedModelId === id) {
      setSelectedModelId(null);
    }
    await loadModels();
  }, [loadModels, selectedModelId]);

  const updateModel = useCallback(async (id: number, updates: {
    name?: string;
    quantization?: string;
    file_path?: string;
  }) => {
    await updateModelService({ id, name: updates.name });
    await loadModels();
  }, [loadModels]);

  return {
    models,
    selectedModel,
    selectedModelId,
    loading,
    error,
    loadModels,
    selectModel,
    addModel,
    removeModel,
    updateModel,
  };
}
