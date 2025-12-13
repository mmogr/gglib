/**
 * Hook for lazy-loading and caching tool support detection for HuggingFace models.
 *
 * This hook provides:
 * - In-memory caching of tool support results across all components
 * - Parallel fetching (all requests fire immediately when components mount)
 * - Deduplication (multiple components requesting the same model share one request)
 *
 * Tool icons typically appear ~1 second after search results load.
 */

import { useState, useEffect } from 'react';
import { getHfToolSupport } from '../services/clients/huggingface';

// Global in-memory cache shared across all hook instances
// Maps model ID -> { supports: boolean, loading: boolean, promise?: Promise }
interface CacheEntry {
  supports: boolean | null;
  loading: boolean;
  promise?: Promise<boolean | null>;
}

const toolSupportCache = new Map<string, CacheEntry>();

export interface UseToolSupportResult {
  /** Whether the model supports tool/function calling. Null if unknown/error. */
  supports: boolean | null;
  /** Whether the fetch is currently in progress */
  loading: boolean;
}

/**
 * Hook to check if a HuggingFace model supports tool/function calling.
 *
 * Results are cached in memory - subsequent calls for the same model
 * return instantly from cache.
 *
 * @param modelId - HuggingFace model ID (e.g., "MaziyarPanahi/Qwen3-4B-GGUF")
 * @returns Object with `supports` (boolean | null) and `loading` state
 *
 * @example
 * ```tsx
 * const { supports, loading } = useToolSupportCache(model.id);
 * if (!loading && supports) {
 *   return <span title="Supports tool calling">🔧</span>;
 * }
 * ```
 */
export function useToolSupportCache(modelId: string | null): UseToolSupportResult {
  const [result, setResult] = useState<UseToolSupportResult>(() => {
    if (!modelId) {
      return { supports: null, loading: false };
    }

    // Check cache on initial render
    const cached = toolSupportCache.get(modelId);
    if (cached) {
      return { supports: cached.supports, loading: cached.loading };
    }

    return { supports: null, loading: true };
  });

  useEffect(() => {
    if (!modelId) {
      setResult({ supports: null, loading: false });
      return;
    }

    // Check if already cached (completed)
    const cached = toolSupportCache.get(modelId);
    if (cached && !cached.loading) {
      setResult({ supports: cached.supports, loading: false });
      return;
    }

    // Check if request is already in flight (deduplicate)
    if (cached?.loading && cached.promise) {
      // Wait for existing request to complete
      cached.promise.then((supports) => {
        setResult({ supports, loading: false });
      });
      return;
    }

    // Start new fetch
    setResult({ supports: null, loading: true });

    const fetchPromise = getHfToolSupport(modelId)
      .then((response) => {
        const supports = response.supports_tool_calling;
        toolSupportCache.set(modelId, { supports, loading: false });
        setResult({ supports, loading: false });
        return supports;
      })
      .catch(() => {
        // Cache failures as null to avoid retry loops
        toolSupportCache.set(modelId, { supports: null, loading: false });
        setResult({ supports: null, loading: false });
        return null;
      });

    // Store promise for deduplication
    toolSupportCache.set(modelId, { supports: null, loading: true, promise: fetchPromise });
  }, [modelId]);

  return result;
}

/**
 * Clear the tool support cache.
 * Useful for testing or when user wants to refresh data.
 */
export function clearToolSupportCache(): void {
  toolSupportCache.clear();
}
