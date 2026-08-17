import { useEffect, useRef } from 'react';

import { getTransport } from '../services/transport';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';
import type { ModelEvent } from '../services/transport/types/events';

/**
 * Reload the model library when *another* client changes it.
 *
 * A tab that makes its own edit already refetches at the call site, which is
 * why the library looked correct while this was missing. What it could not
 * see was a second window or browser tab editing the same daemon: add,
 * remove, rename, retag, capability and upgrade all arrive as `model_added` /
 * `model_updated` / `model_removed` on `/api/events`.
 *
 * Scope is one daemon process. A `gglib model add` in a terminal does not
 * reach here — it is a separate process, and the broadcaster is in-process.
 * Downloads are covered separately, by the download event stream.
 *
 * Events are batched within a window because one user action can produce
 * several: a retag of many models, or a burst of edits from another window.
 * Each burst is a reason to refetch once, not once per event.
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
