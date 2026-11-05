/**
 * The words a state is shown as, one per state, the same words the CLI
 * prints. `gglib_core::domain::state_words` is where they are written;
 * `contracts/vocabulary/words.json` is that table as the CLI's tests record
 * it, and `tests/ts/contracts/stateWords.test.ts` holds these maps to the
 * file. The `Record` types are the generated bindings, so a state the daemon
 * adds fails to compile here until it has a word.
 */

import type { EndpointState } from '../../types/generated/EndpointState';
import type { KeyState } from '../../types/generated/KeyState';
import type { ReachState } from '../../types/generated/ReachState';
import type { ResidencyState } from '../../types/generated/ResidencyState';

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
