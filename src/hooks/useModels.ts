import { useState, useEffect, useCallback } from 'react';
import { GgufModel } from '../types';
import { TauriService } from '../services/tauri';

export function useModels() {
  const [models, setModels] = useState<GgufModel[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadModels = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const modelList = await TauriService.listModels();
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

  const selectModel = useCallback((id: number | null) => {
    setSelectedModelId(id);
  }, []);

  const selectedModel = models.find(m => m.id === selectedModelId) || null;

  const addModel = useCallback(async (filePath: string) => {
    await TauriService.addModel(filePath);
    await loadModels();
  }, [loadModels]);

  const removeModel = useCallback(async (id: number, force: boolean = false) => {
    await TauriService.removeModel(id.toString(), force);
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
    await TauriService.updateModel(id, updates);
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
