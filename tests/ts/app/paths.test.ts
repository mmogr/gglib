/**
 * The addresses, written in one place and read back in one function.
 */

import { describe, it, expect } from 'vitest';

import {
  benchmarkModelIds,
  inspectorSubject,
  libraryIntent,
  paths,
  settingsAt,
  settingsSection,
  withoutSettings,
} from '../../../src/app/paths';

describe('paths', () => {
  it('names each place, and inspectorSubject reads the selection back', () => {
    expect(paths.library()).toBe('/library');
    expect(paths.model(7)).toBe('/models/7');
    expect(paths.far(3)).toBe('/far/3');
    expect(paths.chat(7)).toBe('/chat/7');
    expect(paths.chat(7, 'console')).toBe('/chat/7?view=console');
    expect(paths.farChat(3)).toBe('/chat/far/3');
    expect(paths.benchmarks([9, 11])).toBe('/diagnostics/benchmarks?models=9,11');
    expect(paths.benchmarks()).toBe('/diagnostics/benchmarks');

    expect(inspectorSubject({ pathname: '/models/7' })).toEqual({ kind: 'model', id: 7 });
    expect(inspectorSubject({ pathname: '/far/3' })).toEqual({ kind: 'far', id: 3 });
    expect(inspectorSubject({ pathname: '/library' })).toBeNull();
    expect(inspectorSubject({ pathname: '/chat/7' })).toBeNull();
    expect(inspectorSubject({ pathname: '/models/seven' })).toBeNull();
  });

  it('the settings dialog is a search parameter over the place, opened and closed without moving', () => {
    const at = { pathname: '/models/7', search: '?view=add' };
    const opened = settingsAt(at, 'system');
    expect(opened).toBe('/models/7?view=add&settings=system');
    expect(settingsSection({ search: '?view=add&settings=system' })).toBe('system');
    expect(settingsSection({ search: '?settings=nope' })).toBeNull();
    expect(settingsSection({ search: '' })).toBeNull();
    expect(withoutSettings({ pathname: '/models/7', search: '?view=add&settings=system' })).toBe('/models/7?view=add');
    expect(withoutSettings({ pathname: '/library', search: '?settings=general' })).toBe('/library');
  });

  it("reads the menu's interim intents and the benchmark ids, dropping what is not one", () => {
    expect(libraryIntent('?view=add&sub=browse')).toEqual({ add: 'browse', serve: false });
    expect(libraryIntent('?view=add&sub=add')).toEqual({ add: 'add', serve: false });
    expect(libraryIntent('?view=add')).toEqual({ add: 'browse', serve: false });
    expect(libraryIntent('?serve=1')).toEqual({ add: null, serve: true });
    expect(libraryIntent('')).toEqual({ add: null, serve: false });
    expect(paths.libraryAdd('add')).toBe('/library?view=add&sub=add');
    expect(paths.serve(7)).toBe('/models/7?serve=1');

    expect(benchmarkModelIds('?models=9,11')).toEqual([9, 11]);
    expect(benchmarkModelIds('?models=9,x, 11')).toEqual([9, 11]);
    expect(benchmarkModelIds('')).toEqual([]);
  });
});
