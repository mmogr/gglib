/**
 * A model's explanation (`GET /api/models/:id/explain`): the launch gglib
 * would make and how it would sample, read once per model and profile and
 * again on `reload`. The fit sentence, the rows, the launch line and the
 * Sampling row all read this one answer.
 *
 * Until the first read answers nothing is drawn; a failed read, or a
 * daemon without the route (`null`), is `failed`, and the inspector says
 * the launch is unavailable rather than drawing placeholders for ever
 * (IN-3). A read for
 * a model or profile no longer asked for is dropped.
 *
 * @module inspector/local/useExplain
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { explainModel } from '../../services/transport/api/models/local';
import type { LaunchExplanation } from '../../types';

export interface ExplainState {
  explanation: LaunchExplanation | null;
  failed: boolean;
  reload: () => Promise<void>;
}

export function useExplain(modelId: number, profile: string | null): ExplainState {
  const [explanation, setExplanation] = useState<LaunchExplanation | null>(null);
  const [failed, setFailed] = useState(false);
  const asked = useRef(0);

  const reload = useCallback(async () => {
    const ask = ++asked.current;
    try {
      const read = await explainModel(modelId, profile ?? undefined);
      if (ask !== asked.current) return;
      setExplanation(read);
      setFailed(read === null);
    } catch {
      if (ask !== asked.current) return;
      setExplanation(null);
      setFailed(true);
    }
  }, [modelId, profile]);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { explanation, failed, reload };
}
