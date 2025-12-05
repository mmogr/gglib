/**
 * Batch items within a time window before flushing.
 * 
 * Framework-agnostic utility that collects items pushed within a time window,
 * then auto-flushes them as a batch when the window expires.
 * 
 * @example
 * const batcher = createBatchWithinWindow<string>(350, (items) => {
 *   console.log('Flushed:', items);
 * });
 * 
 * batcher.push('a');
 * batcher.push('b'); // within 350ms of 'a'
 * // After 350ms: logs ['a', 'b']
 * 
 * batcher.dispose(); // cleanup when done
 */

export interface BatchWithinWindow<T> {
  /** Add an item to the current batch. Starts window timer if not already running. */
  push: (item: T) => void;
  /** Immediately flush any pending items and reset the timer. */
  flush: () => void;
  /** Dispose the batcher, clearing any pending timer. Does NOT flush. */
  dispose: () => void;
}

export type BatchFlushCallback<T> = (items: T[]) => void;

/** Default window duration in milliseconds */
export const DEFAULT_BATCH_WINDOW_MS = 350;

/**
 * Create a new batch-within-window utility.
 * 
 * @param windowMs Time window in milliseconds (default: 350ms)
 * @param onFlush Callback invoked when batch is flushed
 */
export function createBatchWithinWindow<T>(
  windowMs: number = DEFAULT_BATCH_WINDOW_MS,
  onFlush: BatchFlushCallback<T>
): BatchWithinWindow<T> {
  let items: T[] = [];
  let timerId: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timerId !== null) {
      clearTimeout(timerId);
      timerId = null;
    }
    if (items.length > 0) {
      const batch = items;
      items = [];
      onFlush(batch);
    }
  };

  const push = (item: T) => {
    items.push(item);
    
    // Start timer on first item, let subsequent items ride the same window
    if (timerId === null) {
      timerId = setTimeout(flush, windowMs);
    }
  };

  const dispose = () => {
    if (timerId !== null) {
      clearTimeout(timerId);
      timerId = null;
    }
    items = [];
  };

  return { push, flush, dispose };
}
