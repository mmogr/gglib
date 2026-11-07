/**
 * Every path the app navigates to, written here and nowhere else.
 *
 * A screen is a place with an address: the library, a model in it (or its
 * log), the paired machine's model, a Hugging Face repo, the endpoint (with
 * a model inspected beside it), a chat (by its conversation, not its model),
 * the devices, the settings, the diagnostics.
 * The sidebar, the native menu, a row's click and a test all go through
 * these, so a path's shape is changed in one file. Reading a path back is
 * `inspectorSubject` and `chatAddress`, the readers of what the address names.
 *
 * @module app/paths
 */

import type { Location } from 'react-router';

/** The settings place's sections, in the order its list shows them. */
export const SETTINGS_SECTIONS = ['general', 'endpoint', 'models', 'tools', 'engines', 'about'] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

export const paths = {
  /** The library with nothing picked. */
  library: () => '/library',
  /** The library with one of this machine's models in the inspector. */
  model: (id: number) => `/models/${id}`,
  /** The library with the log of this machine's model `id` in the inspector. */
  log: (id: number) => `/models/${id}/log`,
  /** The library with the paired machine's model `id` (its id there) in the inspector. */
  far: (id: number) => `/far/${id}`,
  /** The library with the Hugging Face repo `owner/name` in the inspector. */
  hf: (repo: string) => `/get/${repo.split('/').map(encodeURIComponent).join('/')}`,
  /** The endpoint place. */
  endpoint: () => '/endpoint',
  /** The endpoint place with this machine's model `id` in the inspector beside it. */
  endpointInspect: (id: number) => `/endpoint?inspect=model:${id}`,
  /** The endpoint place scrolled to its Connect a client block (the menu's Connect a Client…). */
  endpointConnect: () => '/endpoint?connect=1',
  /** The endpoint place with model `id` inspected and its Load with options sheet opened. */
  endpointServe: (id: number) => `/endpoint?inspect=model:${id}&serve=1`,
  /** This machine's conversation `id`, on the model it names. */
  chat: (id: number) => `/chat/${id}`,
  /**
   * An empty chat: on this machine's model `model`, on the paired machine's
   * model `far` (its id there), or with neither on the model the chat route
   * picks. The first send makes the conversation and the address becomes it.
   */
  newChat: (on: { model?: number; far?: number } = {}) =>
    on.far !== undefined ? `/chat/new?far=${on.far}` : on.model !== undefined ? `/chat/new?model=${on.model}` : '/chat/new',
  /** The paired machine's own conversation `id` (its id there), which that machine runs. */
  farChat: (id: number) => `/chat/far/${id}`,
  /** The library with its search focused for Hugging Face (`browse`), or adding a GGUF file (`add`). */
  libraryAdd: (sub: 'browse' | 'add') => `/library?add=${sub === 'add' ? 'file' : 'browse'}`,
  /** A model's inspector with its Load with options sheet opened. */
  serve: (id: number) => `/models/${id}?serve=1`,
  /** The devices: sharing this Mac, the paired devices, using another machine. */
  devices: () => '/devices',
  /** The settings place, open on `section`. */
  settings: (section: SettingsSection = 'general') => `/settings/${section}`,
  /** The benchmarks, with `modelIds` picked to run. */
  benchmarks: (modelIds: number[] = []) =>
    modelIds.length > 0 ? `/diagnostics/benchmarks?models=${modelIds.join(',')}` : '/diagnostics/benchmarks',
} as const;

/**
 * What the inspector shows: one of this machine's models (`log`: its log),
 * one of the paired machine's, or a Hugging Face repo by its `owner/name`.
 */
export type InspectorSubject =
  | { kind: 'model'; id: number; log: boolean }
  | { kind: 'far'; id: number }
  | { kind: 'hf'; repo: string };

/** A path segment that is a model id: digits, nothing else. */
function idOf(segment: string | undefined): number | null {
  if (segment === undefined || !/^\d+$/.test(segment)) return null;
  return Number(segment);
}

/** A path segment as it was written, or `null` for one that is not text. */
function decoded(segment: string): string | null {
  try {
    return decodeURIComponent(segment);
  } catch {
    return null;
  }
}

/**
 * The subject the address names. In the library it is the path:
 * `/models/:id` is this machine's model and `/models/:id/log` its log,
 * `/far/:id` the paired machine's, `/get/:owner/:name` a Hugging Face repo.
 * Beside another place it is `?inspect=`: on `/endpoint`, `model:<id>` is
 * this machine's model. Anything else is nothing.
 */
export function inspectorSubject(location: Pick<Location, 'pathname'> & Partial<Pick<Location, 'search'>>): InspectorSubject | null {
  const [, place, second, third, ...rest] = location.pathname.split('/');
  if (location.pathname === paths.endpoint()) {
    const inspect = new URLSearchParams(location.search ?? '').get('inspect');
    const id = idOf(inspect?.startsWith('model:') ? inspect.slice('model:'.length) : undefined);
    return id === null ? null : { kind: 'model', id, log: false };
  }
  if (place === 'models') {
    const id = idOf(second);
    if (id === null || rest.length > 0 || (third !== undefined && third !== 'log')) return null;
    return { kind: 'model', id, log: third === 'log' };
  }
  if (place === 'get') {
    const owner = second ? decoded(second) : null;
    const name = third ? decoded(third) : null;
    return owner && name && rest.length === 0 ? { kind: 'hf', repo: `${owner}/${name}` } : null;
  }
  if (place === 'far') {
    const id = idOf(second);
    return id === null ? null : { kind: 'far', id };
  }
  return null;
}

/** The address at `location` without its search parameter `name`, the rest kept. */
export function withoutParam(location: Pick<Location, 'pathname' | 'search'>, name: string): string {
  const params = new URLSearchParams(location.search);
  params.delete(name);
  const search = params.toString();
  return search ? `${location.pathname}?${search}` : location.pathname;
}

/**
 * Where the inspector's × goes from `location`: the endpoint place without
 * its `?inspect=`, else the library with nothing picked.
 */
export function inspectorClosed(location: Pick<Location, 'pathname' | 'search'>): string {
  return location.pathname === paths.endpoint() ? withoutParam(location, 'inspect') : paths.library();
}

/** The settings section a `/settings/:section` segment names, or `null` for one that names none. */
export function settingsSection(segment: string | undefined): SettingsSection | null {
  return SETTINGS_SECTIONS.find((s) => s === segment) ?? null;
}

/**
 * What the native menu or `+ ▾` asked the library to do, carried as search
 * parameters the page acts on once and strips: focus the search for Hugging
 * Face, add a GGUF file, or open the picked model's Load with options sheet
 * (which the inspector reads and strips itself).
 */
export interface LibraryIntent {
  add: 'browse' | 'add' | null;
  serve: boolean;
}

export function libraryIntent(search: string): LibraryIntent {
  const params = new URLSearchParams(search);
  const sub = params.get('add');
  const add = sub === 'file' ? 'add' : sub === 'browse' ? 'browse' : null;
  return { add, serve: params.get('serve') === '1' };
}

/**
 * The model of this machine's the native menu acts on: the one in the
 * inspector (beside the library or the endpoint). `null` elsewhere, where
 * Start, Stop and Remove have nothing to act on; a chat's address names its
 * conversation, not a model.
 */
export function menuModelId(location: Pick<Location, 'pathname'> & Partial<Pick<Location, 'search'>>): number | null {
  const subject = inspectorSubject(location);
  return subject?.kind === 'model' ? subject.id : null;
}

/**
 * The chat the address names: one of this machine's conversations
 * (`/chat/:id`), an empty one (`/chat/new`, on `?model=` or `?far=` when
 * given), or the paired machine's own (`/chat/far/:id`); `null` elsewhere.
 */
export type ChatAddress =
  | { kind: 'saved'; id: number }
  | { kind: 'new'; model: number | null; far: number | null }
  | { kind: 'far'; id: number };

export function chatAddress(location: Pick<Location, 'pathname' | 'search'>): ChatAddress | null {
  const [, place, second, third, ...rest] = location.pathname.split('/');
  if (place !== 'chat' || rest.length > 0) return null;
  if (second === 'far') {
    const id = idOf(third);
    return id === null ? null : { kind: 'far', id };
  }
  if (third !== undefined) return null;
  if (second === 'new') {
    const params = new URLSearchParams(location.search);
    return { kind: 'new', model: idOf(params.get('model') ?? undefined), far: idOf(params.get('far') ?? undefined) };
  }
  const id = idOf(second);
  return id === null ? null : { kind: 'saved', id };
}

/** The model ids `/diagnostics/benchmarks?models=` names, in order, bad ones dropped. */
export function benchmarkModelIds(search: string): number[] {
  const raw = new URLSearchParams(search).get('models') ?? '';
  return raw
    .split(',')
    .map((s) => idOf(s.trim()))
    .filter((id): id is number => id !== null);
}
