/**
 * The hardware-sized suggestion `gglib up` makes on a first run, surfaced
 * where models are actually chosen.
 *
 * The shortlist and the sizing already existed in the domain layer; only the
 * terminal could see them. Browsing HuggingFace with no idea what your machine
 * can hold is the problem this solves — the search results say nothing about
 * whether a 30B will fit in 16 GB.
 */

import { FC, useEffect, useState } from 'react';
import { Button } from '../ui/Button';
import { getRecommendedModel } from '../../services/transport/api/setup';
import type { ModelRecommendation } from '../../types/setup';
import { appLogger } from '../../services/platform';
import { formatMemorySize } from '../../hooks/useSystemMemory';

const BUDGET_COPY: Record<ModelRecommendation['budgetSource'], string> = {
  vram: 'GPU memory',
  unifiedMemory: 'unified memory',
  systemRam: 'system memory',
};

interface RecommendedModelProps {
  /** Put the repo in the search box rather than downloading behind their back. */
  onUseRepo: (repo: string) => void;
}

export const RecommendedModel: FC<RecommendedModelProps> = ({ onUseRepo }) => {
  const [recommendation, setRecommendation] = useState<ModelRecommendation | null>(null);
  const [loaded, setLoaded] = useState(false);
  // A failed request and a successful "nothing fits" are different answers.
  // Rendering the second for the first tells the user their machine is too
  // small on the strength of a network error.
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void getRecommendedModel()
      .then((rec) => {
        if (!cancelled) setRecommendation(rec);
      })
      .catch((err: unknown) => {
        // Advisory: browsing works without it, so a failure stays silent.
        appLogger.warn('component.model', 'Could not read model recommendation', { error: err });
        if (!cancelled) setFailed(true);
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // `null` means nothing in the shortlist fits, which is worth saying — a
  // machine that cannot run the smallest candidate should not be left
  // browsing models it has no chance of loading.
  if (loaded && !recommendation && !failed) {
    return (
      <p className="m-0 py-sm text-xs text-text-muted">
        No model in gglib's shortlist fits this machine's memory. Smaller or more heavily
        quantized models may still work — the fit indicators on each result show what will.
      </p>
    );
  }

  if (!recommendation) return null;

  return (
    <div className="py-sm flex items-baseline justify-between gap-md flex-wrap">
      <div className="min-w-0">
        <p className="m-0 text-xs text-text-secondary">
          Suggested for this machine:{' '}
          <span className="font-mono text-text">{recommendation.repo}</span>
        </p>
        <p className="m-0 text-2xs text-text-muted">
          {recommendation.rationale}. Needs{' '}
          <span className="font-mono tabular-nums">
            {formatMemorySize(recommendation.requiredBytes)}
          </span>{' '}
          of your{' '}
          <span className="font-mono tabular-nums">
            {formatMemorySize(recommendation.budgetBytes)}
          </span>{' '}
          {BUDGET_COPY[recommendation.budgetSource]}.
        </p>
      </div>
      <Button variant="ghost" size="sm" onClick={() => onUseRepo(recommendation.repo)}>
        Find it
      </Button>
    </div>
  );
};
