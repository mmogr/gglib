/**
 * What fills `modelRegistry`.
 *
 * `bridgeEvents` over the `model` category, as `proxyEvents` is over `proxy`:
 * subscribe first, read, and drop a read an event or a cleanup overtook. With
 * one difference: events are batched within a window before the re-read,
 * because one action can produce several (a retag of many models, a burst of
 * edits from another window), and each burst is a reason to read once, not
 * once per event. An event that lands while a read is out supersedes that
 * read through the bridge's version, and schedules the next through the
 * batcher, so the list converges on what the daemon has.
 *
 * Who sends the events: the daemon, for any client's change to the library.
 * A tab that makes its own edit does not wait for them; the actions below
 * read again as soon as the daemon has answered. A `gglib model …` command
 * in a terminal is a separate process, and reaches here when it changes the
 * library through `ModelOps`: it posts the daemon the event its change
 * emitted. Downloads are covered separately, by the download event stream.
 *
 * A screen that keeps more than the list from the library, such as the filter
 * options and the tag list, listens with `onModelLibraryChange` and is told
 * once per burst, as the registry is.
 *
 * @module services/modelEvents
 */

import { bridgeEvents } from './bridgeEvents';
import { applyLibraryRead, readLibrary, resetModelLibrary } from './modelRegistry';
import type { ModelUpdates } from './modelRegistry';
import { getTransport } from './transport';
import type { ModelEvent } from './transport/types/events';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';

type Listener = () => void;

const listeners = new Set<Listener>();

/** Whether the bridge is up: a read asked while it is down writes nothing. */
let live = false;

let batcher = createBatchWithinWindow<ModelEvent>(DEFAULT_BATCH_WINDOW_MS, onBurst);

const bridge = bridgeEvents({
  category: 'model',
  onEvent: (evt) => batcher.push(evt),
  read: readLibrary,
  apply: applyLibraryRead,
  reset: resetModelLibrary,
});

/** One burst of events: read again, and tell whoever keeps more than the list. */
function onBurst(): void {
  void bridge.refresh();
  listeners.forEach((listener) => listener());
}

/**
 * Initialize model event handling: subscribe, then read the library.
 * Safe to call multiple times — only initializes once.
 */
export function initModelEvents(): void {
  live = true;
  bridge.init();
}

/**
 * Cleanup model event handling: unsubscribe, drop a pending burst, and empty
 * the registry. Should be called on app unmount or hot-reload.
 */
export function cleanupModelEvents(): void {
  live = false;
  batcher.dispose();
  batcher = createBatchWithinWindow<ModelEvent>(DEFAULT_BATCH_WINDOW_MS, onBurst);
  bridge.cleanup();
}

/**
 * Read the library again now, while the bridge is up; down, nothing is read
 * and nothing written. A read an event overtook is retried once rather than
 * reported: the read itself worked, and one started now is at least as new
 * as both; a retry overtaken too is left to the burst that overtook it.
 */
export async function refreshModels(): Promise<void> {
  if (!live) return;
  const first = await bridge.refresh();
  if (first === 'superseded' && live) await bridge.refresh();
}

/**
 * Be told once per burst of library events, after the registry's own re-read
 * has been asked for. For state kept beside the list, such as the filter
 * options and the tags, which a change to the library may have moved too.
 */
export function onModelLibraryChange(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Register a model file, then read the library again. */
export async function addModel(filePath: string): Promise<void> {
  await getTransport().addModel({ filePath });
  await refreshModels();
}

/** Remove a model, then read the library again. */
export async function removeModel(id: number): Promise<void> {
  await getTransport().removeModel(id);
  await refreshModels();
}

/** Change a model's facts, then read the library again. */
export async function updateModel(id: number, updates: ModelUpdates): Promise<void> {
  await getTransport().updateModel({
    id,
    name: updates.name,
    quantization: updates.quantization,
    filePath: updates.filePath,
    inferenceDefaults: updates.inferenceDefaults,
    serverDefaults: updates.serverDefaults,
    projectorPath: updates.projectorPath,
    components: updates.components,
  });
  await refreshModels();
}
