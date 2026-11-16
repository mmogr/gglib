/**
 * The model library, once.
 *
 * One store holds this machine's models as `GET /api/models` lists them, and
 * every surface that shows them reads it: the library, the inspector, the
 * chat picker and the settings pickers. Before it, `useModels()` was called
 * twice (the library page and the settings dialog) with independent lists and
 * independent fetches, and a third copy sat in the library's filter hook.
 *
 * What fills it is `modelEvents`: a read on init and on every opening of the
 * event stream (so a first read refused on a cold start is followed by one
 * that lands), a read after each burst of `model_*` events, and a read after
 * each edit made here, through the actions below. A read that failed leaves
 * the last list standing and says why in `error`, and is not newer state: an
 * earlier read that lands after it is still applied. `loaded` turns true at
 * the first read that answered, either way, and stays true, so a refresh
 * with a list already shown does not flash it away; `heard` turns true only
 * at the first list read, for a reader that must not take "no list" for an
 * empty library. The daemon's filtered and sorted lists (`listModels(query)`) are not
 * held here: they are one page's view, asked for with its filters.
 *
 * @module services/modelRegistry
 */

import { createEventStore } from './createEventStore';
import type { GgufModel, ServerConfig, SparseInferenceConfig } from '../types';
import type { ComponentChanges } from './transport/types/models';

/**
 * What an update may change. `projectorPath`: a path links, `null` unlinks,
 * absent leaves the link alone. `components`: the same, by role.
 */
export interface ModelUpdates {
  name?: string;
  quantization?: string;
  filePath?: string;
  inferenceDefaults?: SparseInferenceConfig;
  serverDefaults?: ServerConfig | null;
  projectorPath?: string | null;
  components?: ComponentChanges;
}

export interface ModelLibrary {
  /** The models as last read, in the daemon's order; empty until the first read. */
  models: GgufModel[];
  /** Whether a read has answered yet, with a list or with a failure. */
  loaded: boolean;
  /** Whether a list has been read: until then, no models is not an empty library. */
  heard: boolean;
  /** Why the last read failed, or `null` when it did not. */
  error: string | null;
}

/** What one read came back with: the list, or the reason it did not. */
export type LibraryRead = { models: GgufModel[] } | { failed: string };

const EMPTY: ModelLibrary = { models: [], loaded: false, heard: false, error: null };

const store = createEventStore<ModelLibrary>(EMPTY);

/** Write a read's answer: a new list, or the old one with the failure beside it. */
export function applyLibraryRead(read: LibraryRead): void {
  const current = store.getState();
  if ('models' in read) {
    store.setState({ models: read.models, loaded: true, heard: true, error: null });
  } else {
    store.setState({ ...current, loaded: true, error: read.failed });
  }
}

/** Empty the store, for a bridge that is being taken down. */
export function resetModelLibrary(): void {
  store.setState(EMPTY);
}

/** The library as it stands, for code outside React. */
export function getModelLibrary(): ModelLibrary {
  return store.getState();
}

export interface ModelLibraryView {
  models: GgufModel[];
  /** True until the first read answers. A later refresh is not loading. */
  loading: boolean;
  /** Whether a list has been read; a failed read is not one. */
  heard: boolean;
  error: string | null;
}

/** React hook: the library, re-rendering on every write. */
export function useModelLibrary(): ModelLibraryView {
  const { models, loaded, heard, error } = store.useStore();
  return { models, loading: !loaded, heard, error };
}

/** React hook: one model by id, or `null` when the library has no such row. */
export function useModel(id: number | null): GgufModel | null {
  const { models } = store.useStore();
  return id === null ? null : (models.find((m) => m.id === id) ?? null);
}
