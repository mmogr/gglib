/**
 * A tool server's state as the row draws it: the shape and the word for the
 * status the daemon's server routes answer with (`McpServerStatusDto`).
 *
 * Rust writes no word for these states yet (`gglib_core::domain::state_words`
 * has none for MCP); the words are what `gglib mcp list` prints
 * (`status_and_tools` in `gglib-cli`'s `mcp_cli.rs`), capitalised as a row
 * shows a state. The `Record` is over the generated type, so a state the
 * daemon adds fails to compile here until it has a word. A failed server is
 * the one state that carries its own text: the daemon's reason, said under
 * the row.
 *
 * @module places/settings/tools/serverState
 */

import type { MarkKind } from '../../../components/status/StatusMark';
import type { McpServerInfo, McpServerStatus } from '../../../services/transport';

type NamedStatus = Exclude<McpServerStatus, object>;

/** Each named state's word. */
export const SERVER_STATE_WORDS: Readonly<Record<NamedStatus, string>> = {
  running: 'Running',
  starting: 'Starting',
  stopped: 'Stopped',
  unsupported: 'Not supported yet',
};

const SERVER_STATE_MARKS: Readonly<Record<NamedStatus, MarkKind>> = {
  running: 'on',
  starting: 'changing',
  stopped: 'off',
  unsupported: 'off',
};

/** The word for a server that failed, whose reason is said under the row. */
export const FAILED_WORD = 'Failed';

export interface ServerState {
  kind: MarkKind;
  word: string;
  /** The daemon's reason, for a server that failed. */
  reason: string | null;
}

/** The shape, word and any reason for `info`'s status. */
export function serverState(info: McpServerInfo): ServerState {
  const { status } = info;
  if (typeof status === 'object') return { kind: 'failed', word: FAILED_WORD, reason: status.error };
  return { kind: SERVER_STATE_MARKS[status], word: SERVER_STATE_WORDS[status], reason: null };
}
