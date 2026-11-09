/**
 * A state has one word on every surface. `gglib_core::domain::state_words`
 * writes it (and `ChoiceSource::word` the words for whose choice a launch
 * argument is); `gglib-cli`'s state-words test records every state and
 * source with its wire name and its word in `contracts/vocabulary/words.json`;
 * the GUI's word maps must say the same words for the same states, and no
 * others.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import {
  CHOICE_WORDS,
  ENDPOINT_WORDS,
  ENGINE_WORDS,
  FAST_DOWNLOADS_WORDS,
  KEY_WORDS,
  MODELS_DIR_WORDS,
  REACH_WORDS,
  RESIDENCY_WORDS,
  TOOL_SERVER_WORDS,
} from '../../../src/components/status/words';

import { rust } from './rustSource';

interface Recorded {
  state: string;
  word: string;
}

const WORDS = JSON.parse(rust('contracts/vocabulary/words.json')) as {
  endpoint: Recorded[];
  residency: Recorded[];
  reach: Recorded[];
  key: Recorded[];
  choice: Recorded[];
  engine: Recorded[];
  tool_server: Recorded[];
  fast_downloads: Recorded[];
  models_dir: Recorded[];
};

function asRows(map: Readonly<Record<string, string>>): Recorded[] {
  return Object.entries(map).map(([state, word]) => ({ state, word }));
}

describe('the state words are the ones the CLI records', () => {
  it.each([
    ['endpoint', ENDPOINT_WORDS, WORDS.endpoint],
    ['residency', RESIDENCY_WORDS, WORDS.residency],
    ['reach', REACH_WORDS, WORDS.reach],
    ['key', KEY_WORDS, WORDS.key],
    ['choice', CHOICE_WORDS, WORDS.choice],
    ['engine', ENGINE_WORDS, WORDS.engine],
    ['tool_server', TOOL_SERVER_WORDS, WORDS.tool_server],
    ['fast_downloads', FAST_DOWNLOADS_WORDS, WORDS.fast_downloads],
    ['models_dir', MODELS_DIR_WORDS, WORDS.models_dir],
  ] as const)('%s: the same states, in the same order, with the same words', (_name, map, recorded) => {
    expect(asRows(map)).toEqual(recorded);
  });

  it('every table the file records has a map here', () => {
    expect(Object.keys(WORDS)).toEqual([
      'endpoint',
      'residency',
      'reach',
      'key',
      'choice',
      'engine',
      'tool_server',
      'fast_downloads',
      'models_dir',
    ]);
  });
});
