import { useState, useEffect, useCallback } from 'react';
import type { SamplingExplanation } from '../../../types';
import { explainModelSampling } from '../../../services/transport/api/models/local';
import { appLogger } from '../../../services/platform';

export interface SamplingExplanationState {
  explanation: SamplingExplanation | null;
  isLoading: boolean;
  /** The fetch failed; render a fallback rather than stale rows. */
  hasError: boolean;
  reload: () => Promise<void>;
}

/**
 * Fetch a model's resolved sampling parameters and their provenance.
 *
 * Refetches when the selected model or profile changes, and when `refreshKey`
 * does — pass the model's stored defaults so saving an edit re-resolves rather
 * than leaving the panel describing the previous configuration.
 */
export function useSamplingExplanation(
  modelId: number | undefined,
  profileName: string | null,
  refreshKey?: unknown,
): SamplingExplanationState {
  const [explanation, setExplanation] = useState<SamplingExplanation | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [hasError, setHasError] = useState(false);

  const reload = useCallback(async () => {
    if (!modelId) return;
    setIsLoading(true);
    setHasError(false);
    try {
      setExplanation(await explainModelSampling(modelId, profileName ?? undefined));
    } catch (error) {
      // Not error-level: hasError already renders a designed fallback, and a
      // backend without the explain route resolves to null before this catch.
      appLogger.warn('hook.ui', 'Failed to load sampling explanation', {
        error,
        modelId,
        profileName,
      });
      setExplanation(null);
      setHasError(true);
    } finally {
      setIsLoading(false);
    }
  }, [modelId, profileName]);

  useEffect(() => {
    if (modelId) {
      reload();
    } else {
      setExplanation(null);
    }
    // `refreshKey` is a caller-supplied invalidation signal, not a value this
    // hook reads.
  }, [modelId, reload, refreshKey]);

  return { explanation, isLoading, hasError, reload };
}
