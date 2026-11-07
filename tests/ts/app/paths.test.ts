/**
 * The addresses, written in one place and read back in one function.
 */

import { describe, it, expect } from 'vitest';

import {
  benchmarkModelIds,
  inspectorSubject,
  libraryIntent,
  menuModelId,
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
    expect(paths.log(7)).toBe('/models/7/log');
    expect(paths.hf('owner/Qwen3-8B-GGUF')).toBe('/get/owner/Qwen3-8B-GGUF');
    expect(paths.farChat(3)).toBe('/chat/far/3');
    expect(paths.benchmarks([9, 11])).toBe('/diagnostics/benchmarks?models=9,11');
    expect(paths.benchmarks()).toBe('/diagnostics/benchmarks');

    expect(inspectorSubject({ pathname: '/models/7' })).toEqual({ kind: 'model', id: 7, log: false });
    expect(inspectorSubject({ pathname: '/models/7/log' })).toEqual({ kind: 'model', id: 7, log: true });
    expect(inspectorSubject({ pathname: '/models/7/other' })).toBeNull();
    expect(inspectorSubject({ pathname: '/get/owner/Qwen3-8B-GGUF' })).toEqual({ kind: 'hf', repo: 'owner/Qwen3-8B-GGUF' });
    expect(inspectorSubject({ pathname: paths.hf('o w/n%') })).toEqual({ kind: 'hf', repo: 'o w/n%' });
    expect(inspectorSubject({ pathname: '/get/owner' })).toBeNull();
    expect(inspectorSubject({ pathname: '/get/owner/name/more' })).toBeNull();
    expect(inspectorSubject({ pathname: '/far/3' })).toEqual({ kind: 'far', id: 3 });
    expect(inspectorSubject({ pathname: '/library' })).toBeNull();
    expect(inspectorSubject({ pathname: '/chat/7' })).toBeNull();
    expect(inspectorSubject({ pathname: '/models/seven' })).toBeNull();
  });

  it("menuModelId is the inspector's model or the open chat's, and nothing elsewhere", () => {
    expect(menuModelId({ pathname: '/models/7' })).toBe(7);
    expect(menuModelId({ pathname: '/models/7/log' })).toBe(7);
    expect(menuModelId({ pathname: '/get/owner/name' })).toBeNull();
    expect(menuModelId({ pathname: '/chat/7' })).toBe(7);
    expect(menuModelId({ pathname: '/chat/far/3' })).toBeNull();
    expect(menuModelId({ pathname: '/far/3' })).toBeNull();
    expect(menuModelId({ pathname: '/library' })).toBeNull();
    expect(menuModelId({ pathname: '/chat/7/log' })).toBeNull();
  });

  it('the settings dialog is a search parameter over the place, opened and closed without moving', () => {
    const at = { pathname: '/models/7', search: '?serve=1' };
    const opened = settingsAt(at, 'system');
    expect(opened).toBe('/models/7?serve=1&settings=system');
    expect(settingsSection({ search: '?serve=1&settings=system' })).toBe('system');
    expect(settingsSection({ search: '?settings=nope' })).toBeNull();
    expect(settingsSection({ search: '' })).toBeNull();
    expect(withoutSettings({ pathname: '/models/7', search: '?serve=1&settings=system' })).toBe('/models/7?serve=1');
    expect(withoutSettings({ pathname: '/library', search: '?settings=general' })).toBe('/library');
  });

  it("reads the library's intents and the benchmark ids, dropping what is not one", () => {
    expect(libraryIntent(paths.libraryAdd('browse').slice('/library'.length))).toEqual({ add: 'browse', serve: false });
    expect(libraryIntent(paths.libraryAdd('add').slice('/library'.length))).toEqual({ add: 'add', serve: false });
    expect(libraryIntent('?add=other')).toEqual({ add: null, serve: false });
    expect(libraryIntent('?serve=1')).toEqual({ add: null, serve: true });
    expect(libraryIntent('')).toEqual({ add: null, serve: false });
    expect(paths.libraryAdd('add')).toBe('/library?add=file');
    expect(paths.libraryAdd('browse')).toBe('/library?add=browse');
    expect(paths.serve(7)).toBe('/models/7?serve=1');

    expect(benchmarkModelIds('?models=9,11')).toEqual([9, 11]);
    expect(benchmarkModelIds('?models=9,x, 11')).toEqual([9, 11]);
    expect(benchmarkModelIds('')).toEqual([]);
  });
});
