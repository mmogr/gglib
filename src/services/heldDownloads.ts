/**
 * Downloads held by the page: a Download (or a Retry) whose queue request
 * Hugging Face's rate limit refused, kept until the limit lifts and the
 * request is sent again (`downloadsEvents`). The library shows each one as
 * a row among the queue's, with × to drop it.
 *
 * They live in this page's memory, not the daemon's queue: a reload, or a
 * bridge taken down, loses them.
 *
 * @module services/heldDownloads
 */

import { createEventStore } from './createEventStore';

/** One download held until the Hub's limit lifts. */
export interface HeldDownload {
  /**
   * The id its download will have: `repo:QUANT`, or the repo alone when no
   * quantization is named; a failed download's own id for a Retry.
   */
  id: string;
  /** The repository, as the queue takes it. */
  repo: string;
  quantization?: string;
  /** When the limit lifts (epoch ms) and the request is sent again; `null` while it is being sent, when it can no longer be dropped. */
  liftsAt: number | null;
}

const store = createEventStore<HeldDownload[]>([]);

/** How each held download's wait is stopped, by its id. */
const stops = new Map<string, () => void>();

/** The id a queue request's download will have, as a held one is keyed. */
export function heldId(repo: string, quantization?: string): string {
  return quantization ? `${repo}:${quantization}` : repo;
}

/** Hold `held`, or move its time on when it is held already; `stop` ends its wait. */
export function holdDownload(held: HeldDownload, stop: () => void): void {
  stops.set(held.id, stop);
  const rest = store.getState().filter((h) => h.id !== held.id);
  const at = store.getState().findIndex((h) => h.id === held.id);
  store.setState(at === -1 ? [...rest, held] : [...rest.slice(0, at), held, ...rest.slice(at)]);
}

/** Held download `id`'s request is being sent again: it shows so, and is not offered to be dropped. */
export function markSending(id: string): void {
  const state = store.getState();
  if (!state.some((h) => h.id === id && h.liftsAt !== null)) return;
  store.setState(state.map((h) => (h.id === id ? { ...h, liftsAt: null } : h)));
}

/** Forget `id`: it was queued, dropped, or failed. */
export function releaseHeld(id: string): void {
  stops.delete(id);
  if (!store.getState().some((h) => h.id === id)) return;
  store.setState(store.getState().filter((h) => h.id !== id));
}

/** Drop held download `id`: its wait ends and nothing is sent. */
export function dropHeld(id: string): void {
  stops.get(id)?.();
  releaseHeld(id);
}

/** Drop every held download, for a bridge that is being taken down. */
export function dropAllHeld(): void {
  for (const id of [...stops.keys()]) dropHeld(id);
  store.setState([]);
}

/** React hook: the downloads held, in the order they were clicked. */
export function useHeldDownloads(): HeldDownload[] {
  return store.useStore();
}
