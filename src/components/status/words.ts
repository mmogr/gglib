/**
 * The words a state is shown as, one per state, the same words the CLI
 * prints, and the words for whose choice a launch argument is: the
 * endpoint, a model's residency, a paired machine's reach, the key rule, an
 * engine's install, a tool server, the download accelerator and the models
 * folder.
 * `gglib_core::domain::state_words` is where the state words are written,
 * `ChoiceSource::word` the source words;
 * `contracts/vocabulary/words.json` is that table as the CLI's tests record
 * it, and `tests/ts/contracts/stateWords.test.ts` holds these maps to the
 * file. The `Record` types are the generated bindings, so a state the daemon
 * adds fails to compile here until it has a word.
 */

import type { ChoiceSource } from '../../types/generated/ChoiceSource';
import type { EndpointState } from '../../types/generated/EndpointState';
import type { EngineState } from '../../types/generated/EngineState';
import type { FastDownloadsState } from '../../types/generated/FastDownloadsState';
import type { KeyState } from '../../types/generated/KeyState';
import type { McpState } from '../../types/generated/McpState';
import type { ModelsDirState } from '../../types/generated/ModelsDirState';
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

/** Whose choice an argument of a launch is: the model's author, gglib, or you. */
export const CHOICE_WORDS: Readonly<Record<ChoiceSource, string>> = {
  model_author: 'model author',
  gglib: 'gglib',
  you: 'you',
};

/** Whether an engine (llama.cpp, stable-diffusion.cpp) is installed, and works. */
export const ENGINE_WORDS: Readonly<Record<EngineState, string>> = {
  installed: 'Installed',
  not_working: 'Installed, not working',
  not_installed: 'Not installed',
};

/** What a tool server (an MCP server) is doing. */
export const TOOL_SERVER_WORDS: Readonly<Record<McpState, string>> = {
  running: 'Running',
  starting: 'Starting',
  stopped: 'Stopped',
  failed: 'Failed',
  unsupported: 'Not supported yet',
};

/** Whether the download accelerator (hf_xet) is set up. */
export const FAST_DOWNLOADS_WORDS: Readonly<Record<FastDownloadsState, string>> = {
  on: 'On',
  off: 'Off',
};

/** Whether the models folder is there to write downloads into. */
export const MODELS_DIR_WORDS: Readonly<Record<ModelsDirState, string>> = {
  writable: 'Writable',
  absent: 'Will be created',
  not_writable: 'Not writable',
};
