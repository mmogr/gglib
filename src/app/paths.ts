/**
 * Every path the app navigates to, written here and nowhere else.
 *
 * A screen is a place with an address: the library, a model in it, the
 * paired machine's model, a chat on a model, the settings, the diagnostics.
 * The sidebar, the native menu, a row's click and a test all go through
 * these, so a path's shape is changed in one file. Reading a path back is
 * `inspectorSubject`, the one reader of what the address names.
 *
 * @module app/paths
 */

import type { Location } from 'react-router';

export type SettingsSection = 'general' | 'profiles' | 'mcp' | 'system';

export const paths = {
  /** The library with nothing picked. */
  library: () => '/library',
  /** The library with one of this machine's models in the inspector. */
  model: (id: number) => `/models/${id}`,
  /** The library with the paired machine's model `id` (its id there) in the inspector. */
  far: (id: number) => `/far/${id}`,
  /** A chat on this machine's model `id`, served here; `view` opens the Console. */
  chat: (modelId: number, view: 'chat' | 'console' = 'chat') =>
    view === 'console' ? `/chat/${modelId}?view=console` : `/chat/${modelId}`,
  /** A chat on the paired machine's model `id` (its id there). */
  farChat: (id: number) => `/chat/far/${id}`,
  /** The library with its search focused for Hugging Face (`browse`), or adding a GGUF file (`add`). */
  libraryAdd: (sub: 'browse' | 'add') => `/library?add=${sub === 'add' ? 'file' : 'browse'}`,
  /** A model's inspector with its Serve dialog opened. */
  serve: (id: number) => `/models/${id}?serve=1`,
  /** The benchmarks, with `modelIds` picked to run. */
  benchmarks: (modelIds: number[] = []) =>
    modelIds.length > 0 ? `/diagnostics/benchmarks?models=${modelIds.join(',')}` : '/diagnostics/benchmarks',
} as const;

/** What the inspector shows: one of this machine's models, or one of the paired machine's. */
export type InspectorSubject = { kind: 'model'; id: number } | { kind: 'far'; id: number };

/** A path segment that is a model id: digits, nothing else. */
function idOf(segment: string | undefined): number | null {
  if (segment === undefined || !/^\d+$/.test(segment)) return null;
  return Number(segment);
}

/**
 * The subject the address names, read from the path: `/models/:id` is this
 * machine's model, `/far/:id` the paired machine's, anything else nothing.
 */
export function inspectorSubject(location: Pick<Location, 'pathname'>): InspectorSubject | null {
  const [, place, second] = location.pathname.split('/');
  if (place === 'models') {
    const id = idOf(second);
    return id === null ? null : { kind: 'model', id };
  }
  if (place === 'far') {
    const id = idOf(second);
    return id === null ? null : { kind: 'far', id };
  }
  return null;
}

/**
 * The settings dialog, open on `section`, over the place at `location`: a
 * search parameter, not a path, because a dialog is drawn over a place and
 * the place stays. (Settings becomes a place of its own with its redesign.)
 */
export function settingsAt(location: Pick<Location, 'pathname' | 'search'>, section: SettingsSection): string {
  const params = new URLSearchParams(location.search);
  params.set('settings', section);
  return `${location.pathname}?${params.toString()}`;
}

/** The place at `location` with the settings dialog closed. */
export function withoutSettings(location: Pick<Location, 'pathname' | 'search'>): string {
  const params = new URLSearchParams(location.search);
  params.delete('settings');
  const search = params.toString();
  return search ? `${location.pathname}?${search}` : location.pathname;
}

const SETTINGS_SECTIONS: readonly SettingsSection[] = ['general', 'profiles', 'mcp', 'system'];

/** The settings section the address opens the dialog on, or `null` with it closed. */
export function settingsSection(location: Pick<Location, 'search'>): SettingsSection | null {
  const section = new URLSearchParams(location.search).get('settings');
  return SETTINGS_SECTIONS.find((s) => s === section) ?? null;
}

/**
 * What the native menu or `+ ▾` asked the library to do, carried as search
 * parameters the page acts on once and strips: focus the search for Hugging
 * Face, add a GGUF file, or open the picked model's Serve dialog (interim,
 * until the Load sheet takes it over).
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
 * inspector, or the one an open chat is on. `null` elsewhere, where Start,
 * Stop and Remove have nothing to act on.
 */
export function menuModelId(location: Pick<Location, 'pathname'>): number | null {
  const subject = inspectorSubject(location);
  if (subject?.kind === 'model') return subject.id;
  const [, place, second, third] = location.pathname.split('/');
  return place === 'chat' && second !== 'far' && third === undefined ? idOf(second) : null;
}

/** The model ids `/diagnostics/benchmarks?models=` names, in order, bad ones dropped. */
export function benchmarkModelIds(search: string): number[] {
  const raw = new URLSearchParams(search).get('models') ?? '';
  return raw
    .split(',')
    .map((s) => idOf(s.trim()))
    .filter((id): id is number => id !== null);
}
