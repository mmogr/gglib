import { useEffect, useRef } from 'react';

import { getTransport } from '../services/transport';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';
import type { ModelEvent } from '../services/transport/types/events';

/**
 * Reload the model library when *another* client changes it.
 *
 * A tab that makes its own edit already refetches at the call site, which is
 * why the library looked correct while this was missing. What it could not
 * see was everything else: a `gglib model add` in a terminal, a second
 * window, a download the daemon registered. Those arrive as `model_added` /
 * `model_updated` / `model_removed` on `/api/events`.
 *
 * Events are batched within a window because bulk operations arrive as a
 * burst — importing a directory emits one event per model, and each is a
 * reason to refetch once, not N times.
 */
export function useModelLibraryEvents(
  onChange: () => void,
  windowMs: number = DEFAULT_BATCH_WINDOW_MS
): void {
  // A ref so a caller passing an inline callback does not tear down and
  // rebuild the subscription on every render.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    const batcher = createBatchWithinWindow<ModelEvent>(windowMs, () => {
      onChangeRef.current();
    });

    const unsubscribe = getTransport().subscribe('model', (event) => {
      batcher.push(event);
    });

    return () => {
      unsubscribe();
      batcher.dispose();
    };
  }, [windowMs]);
}
