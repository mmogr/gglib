/**
 * The words a state is shown as, one per state, the same words the CLI
 * prints, the words for whose choice a launch argument is, and a file
 * check's verdict and each file's health.
 * `gglib_core::domain::state_words` is where the state words are written,
 * `ChoiceSource::word` the source words, `OverallHealth::word` and
 * `ShardHealth::word` the file check's;
 * `contracts/vocabulary/words.json` is that table as the CLI's tests record
 * it, and `tests/ts/contracts/stateWords.test.ts` holds these maps to the
 * file. The `Record` types are the generated bindings, so a state the daemon
 * adds fails to compile here until it has a word.
 */

import type { ChoiceSource } from '../../types/generated/ChoiceSource';
import type { EndpointState } from '../../types/generated/EndpointState';
import type { KeyState } from '../../types/generated/KeyState';
import type { OverallHealth } from '../../types/generated/OverallHealth';
import type { ReachState } from '../../types/generated/ReachState';
import type { ResidencyState } from '../../types/generated/ResidencyState';
import type { ShardHealth } from '../../types/generated/ShardHealth';

/** What the endpoint (the proxy) is doing. */
export const ENDPOINT_WORDS: Readonly<Record<EndpointState, string>> = {
  on: 'On',
  off: 'Off',
  pinned: 'Pinned',
  starting: 'Starting',
};

/** Whether a model is in memory, and what it is doing there. */
export const RESIDENCY_WORDS: Readonly<Record<ResidencyState, string>> = {
  loaded: 'Loaded',
  loading: 'Loading',
  working: 'Working',
  reading: 'Reading',
  waiting: 'Waiting',
  not_loaded: 'Not loaded',
};

/** Whether a paired machine can be reached from here. */
export const REACH_WORDS: Readonly<Record<ReachState, string>> = {
  connected: 'Connected',
  away: 'Away',
  disconnected: 'Disconnected',
};

/** Whether a request to the endpoint must carry the API key. */
export const KEY_WORDS: Readonly<Record<KeyState, string>> = {
  required: 'Required',
  none_needed: 'None needed',
};

/** Whose choice an argument of a launch is: the model's author, gglib, or you. */
export const CHOICE_WORDS: Readonly<Record<ChoiceSource, string>> = {
  model_author: 'model author',
  gglib: 'gglib',
  you: 'you',
};

/** A file check's verdict on a model: `gglib model verify`'s Overall Health. */
export const VERDICT_WORDS: Readonly<Record<OverallHealth, string>> = {
  healthy: 'Healthy',
  unhealthy: 'Unhealthy',
  unverifiable: 'Unverifiable',
};

/** One file's health after a file check, as `gglib model verify --per-shard` prints it. */
export const FILE_HEALTH_WORDS: Readonly<Record<ShardHealth['type'], string>> = {
  healthy: 'Healthy',
  corrupt: 'Corrupt',
  missing: 'Missing',
  no_oid: 'No OID available',
};
