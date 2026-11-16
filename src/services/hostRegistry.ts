/**
 * The machine the service runs on, as `GET /api/version` reports it
 * (`host`, from `gglib_core::host`): its OS, what it is called, and the
 * characters its paths are split on. `hostEvents` keeps it current
 * (`keptCurrent`: read now and each time the event stream opens); the last
 * host stays while the service is gone.
 *
 * Every word the app names that machine with comes from here: `thisMachine`
 * says "This Mac", "This PC" or "This computer" from the service's own word
 * (`machine`), and `fileName` splits a path on the service's separators
 * (or a Hugging Face repository's, `HF_REPO`): the one file-name helper. A
 * browser tab on another machine reads the same answer, so the words are the
 * service's machine's, never the browser's. Before the first answer the
 * machine is "this computer", which is true of any, and a path is shown
 * whole.
 *
 * @module services/hostRegistry
 */

import { createEventStore } from './createEventStore';
import type { Host } from '../types/generated/Host';

const store = createEventStore<Host | null>(null);

/** The host as last read; `null` before the first answer. */
export const currentHost = (): Host | null => store.getState();

/** Keep `host` as the service's: from its answer, or a test's fake one. */
export function applyHost(host: Host | null): void {
  store.setState(host);
}

/** The host, followed: `null` until the first answer lands. */
export const useHost = (): Host | null => store.useStore();

/** The service's machine at the start of a line (`title`) and inside one (`phrase`). */
export interface MachineWords {
  /** "This Mac", "This PC", "This computer". */
  title: string;
  /** "this Mac", "this PC", "this computer". */
  phrase: string;
}

/** What the service's machine is called, from `host`'s word alone. */
export function thisMachine(host: Host | null): MachineWords {
  const machine = host?.machine ?? 'computer';
  return { title: `This ${machine}`, phrase: `this ${machine}` };
}

/** `thisMachine`, followed. */
export const useThisMachine = (): MachineWords => thisMachine(useHost());

/** Where a path's separators are said: the service's host, or a Hugging Face repository. */
export type PathsOf = Pick<Host, 'separators'>;

/** A Hugging Face repository's paths, split on `/` whatever machine reads them. */
export const HF_REPO: PathsOf = { separators: '/' };

/**
 * The last name in `path`, split on the separators of the machine (or the
 * repository) it is a path on, a trailing one ignored; the whole path when it
 * has no name, or before the host is known.
 */
export function fileName(path: string, on: PathsOf | null): string {
  if (on === null) return path;
  const isSeparator = (c: string) => on.separators.includes(c);
  let end = path.length;
  while (end > 0 && isSeparator(path[end - 1])) end -= 1;
  let start = end;
  while (start > 0 && !isSeparator(path[start - 1])) start -= 1;
  return path.slice(start, end) || path;
}
