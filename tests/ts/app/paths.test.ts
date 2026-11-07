/**
 * The addresses, written in one place and read back in one function.
 */

import { describe, it, expect } from 'vitest';

import {
  benchmarkModelIds,
  inspectorClosed,
  chatAddress,
  inspectorSubject,
  libraryIntent,
  menuModelId,
  paths,
  settingsSection,
  withoutParam,
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
    expect(paths.newChat()).toBe('/chat/new');
    expect(paths.newChat({ model: 9 })).toBe('/chat/new?model=9');
    expect(paths.newChat({ far: 3 })).toBe('/chat/new?far=3');
    expect(paths.benchmarks([9, 11])).toBe('/diagnostics/benchmarks?models=9,11');
    expect(paths.benchmarks()).toBe('/diagnostics/benchmarks');
    expect(paths.diagnostics()).toBe('/diagnostics/live');
    expect(paths.diagnostics('system')).toBe('/diagnostics/system');
    expect(paths.devices()).toBe('/devices');

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

  it('beside the endpoint, the subject is ?inspect=model:<id>, and × drops only that', () => {
    expect(paths.endpoint()).toBe('/endpoint');
    expect(paths.endpointInspect(7)).toBe('/endpoint?inspect=model:7');
    expect(paths.endpointServe(7)).toBe('/endpoint?inspect=model:7&serve=1');
    expect(inspectorSubject({ pathname: '/endpoint', search: '?inspect=model:7' })).toEqual({ kind: 'model', id: 7, log: false });
    expect(inspectorSubject({ pathname: '/endpoint', search: '?inspect=model:7&serve=1' })).toEqual({ kind: 'model', id: 7, log: false });
    expect(inspectorSubject({ pathname: '/endpoint', search: '' })).toBeNull();
    expect(inspectorSubject({ pathname: '/endpoint' })).toBeNull();
    expect(inspectorSubject({ pathname: '/endpoint', search: '?inspect=model:seven' })).toBeNull();
    expect(inspectorSubject({ pathname: '/endpoint', search: '?inspect=far:3' })).toBeNull();
    // Only the endpoint reads it: the library's subject is its path.
    expect(inspectorSubject({ pathname: '/library', search: '?inspect=model:7' })).toBeNull();
    expect(menuModelId({ pathname: '/endpoint', search: '?inspect=model:7' })).toBe(7);

    expect(inspectorClosed({ pathname: '/endpoint', search: '?inspect=model:7&x=1' })).toBe('/endpoint?x=1');
    expect(inspectorClosed({ pathname: '/models/7', search: '' })).toBe('/library');
    expect(withoutParam({ pathname: '/endpoint', search: '?inspect=model:7&serve=1' }, 'serve')).toBe('/endpoint?inspect=model%3A7');
  });

  it("menuModelId is the inspector's model, and nothing elsewhere: a chat's address names a conversation", () => {
    expect(menuModelId({ pathname: '/models/7' })).toBe(7);
    expect(menuModelId({ pathname: '/models/7/log' })).toBe(7);
    expect(menuModelId({ pathname: '/get/owner/name' })).toBeNull();
    expect(menuModelId({ pathname: '/chat/7' })).toBeNull();
    expect(menuModelId({ pathname: '/chat/far/3' })).toBeNull();
    expect(menuModelId({ pathname: '/far/3' })).toBeNull();
    expect(menuModelId({ pathname: '/library' })).toBeNull();
    expect(menuModelId({ pathname: '/chat/7/log' })).toBeNull();
  });

  it('chatAddress reads a chat back: a saved conversation, an empty one on a model, the far machine\'s own', () => {
    expect(chatAddress({ pathname: '/chat/7', search: '' })).toEqual({ kind: 'saved', id: 7 });
    expect(chatAddress({ pathname: '/chat/new', search: '' })).toEqual({ kind: 'new', model: null, far: null });
    expect(chatAddress({ pathname: '/chat/new', search: '?model=9' })).toEqual({ kind: 'new', model: 9, far: null });
    expect(chatAddress({ pathname: '/chat/new', search: '?far=3' })).toEqual({ kind: 'new', model: null, far: 3 });
    expect(chatAddress({ pathname: '/chat/new', search: '?model=x' })).toEqual({ kind: 'new', model: null, far: null });
    expect(chatAddress({ pathname: '/chat/far/3', search: '' })).toEqual({ kind: 'far', id: 3 });
    expect(chatAddress({ pathname: '/chat/far/x', search: '' })).toBeNull();
    expect(chatAddress({ pathname: '/chat/7/log', search: '' })).toBeNull();
    expect(chatAddress({ pathname: '/chat/seven', search: '' })).toBeNull();
    expect(chatAddress({ pathname: '/models/7', search: '' })).toBeNull();
  });

  it('settings is a place with a section in its path, General when none is named', () => {
    expect(paths.settings()).toBe('/settings/general');
    expect(paths.settings('engines')).toBe('/settings/engines');
    for (const section of ['general', 'endpoint', 'models', 'tools', 'engines', 'about'] as const) {
      expect(settingsSection(paths.settings(section).split('/')[2])).toBe(section);
    }
    expect(settingsSection('system')).toBeNull();
    expect(settingsSection(undefined)).toBeNull();
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
