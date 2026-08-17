import { useState, useEffect, useCallback, useRef } from 'react';
import { GgufModel, InferenceConfig, ServerConfig } from '../types';
// TRANSPORT_EXCEPTION: setSelectedModel is desktop-only (menu sync)
import { setSelectedModel, appLogger } from '../services/platform';
import { getTransport } from '../services/transport';

export function useModels() {
  const [models, setModels] = useState<GgufModel[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Which fetch is the newest. Reloads used to come only from awaited,
  // sequential mutation handlers; they now also arrive from library events,
  // so two can be in flight at once. Without this, a slow first response
  // landing after a fast second one would overwrite fresh state with stale —
  // and since no further event is coming, the list would stay wrong.
  const requestGeneration = useRef(0);

  const loadModels = useCallback(async () => {
    const generation = ++requestGeneration.current;
    try {
      setLoading(true);
      setError(null);
      const modelList = await getTransport().listModels();
      if (generation !== requestGeneration.current) return;
      setModels(modelList);
    } catch (err) {
      if (generation !== requestGeneration.current) return;
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to load models: ${errorMessage}`);
    } finally {
      if (generation === requestGeneration.current) {
        setLoading(false);
      }
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
      appLogger.warn('hook.models', 'Failed to sync model selection with menu', { error: err });
    });
  }, []);

  const selectedModel = models.find(m => m.id === selectedModelId) || null;

  const addModel = useCallback(async (filePath: string) => {
    await getTransport().addModel({ filePath });
    await loadModels();
  }, [loadModels]);

  const removeModel = useCallback(async (id: number, _force: boolean = false) => {
    // Note: 'force' param not supported by Transport - caller should handle confirmation
    await getTransport().removeModel(id);
    if (selectedModelId === id) {
      setSelectedModelId(null);
    }
    await loadModels();
  }, [loadModels, selectedModelId]);

  const updateModel = useCallback(async (id: number, updates: {
    name?: string;
    quantization?: string;
    filePath?: string;
    inferenceDefaults?: InferenceConfig;
    serverDefaults?: ServerConfig | null;
  }) => {
    await getTransport().updateModel({ 
      id, 
      name: updates.name,
      quantization: updates.quantization,
      filePath: updates.filePath,
      inferenceDefaults: updates.inferenceDefaults,
      serverDefaults: updates.serverDefaults,
    });
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
