/**
 * A state has one word on every surface. `gglib_core::domain::state_words`
 * writes it (`ChoiceSource::word` the words for whose choice a launch
 * argument is, `OverallHealth::word` and `ShardHealth::word` a file check's
 * verdict and each file's health); `gglib-cli`'s state-words test records
 * every state and source with its wire name and its word in
 * `contracts/vocabulary/words.json`;
 * the GUI's word maps must say the same words for the same states, and no
 * others.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import {
  CHOICE_WORDS,
  ENDPOINT_WORDS,
  FILE_HEALTH_WORDS,
  KEY_WORDS,
  REACH_WORDS,
  RESIDENCY_WORDS,
  VERDICT_WORDS,
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
  verdict: Recorded[];
  file_health: Recorded[];
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
    ['verdict', VERDICT_WORDS, WORDS.verdict],
    ['file_health', FILE_HEALTH_WORDS, WORDS.file_health],
  ] as const)('%s: the same states, in the same order, with the same words', (_name, map, recorded) => {
    expect(asRows(map)).toEqual(recorded);
  });
});
