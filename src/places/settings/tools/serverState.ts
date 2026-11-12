/**
 * A tool server's state as the row draws it: the shape for the state the
 * daemon's server routes answer with (`McpServerInfo.state`) and its word,
 * `TOOL_SERVER_WORDS`, the word `gglib mcp list` prints. A failed server is
 * the one state that carries its own text: the daemon's reason, from its
 * status, said under the row. The `Record` is over the generated type, so a
 * state the daemon adds fails to compile here until it has a shape.
 *
 * @module places/settings/tools/serverState
 */

import type { MarkKind } from '../../../components/status/StatusMark';
import { TOOL_SERVER_WORDS } from '../../../components/status/words';
import type { McpServerInfo } from '../../../services/transport';
import type { McpState } from '../../../types/generated/McpState';

const SERVER_STATE_MARKS: Readonly<Record<McpState, MarkKind>> = {
  running: 'on',
  starting: 'changing',
  stopped: 'off',
  failed: 'failed',
};

export interface ServerState {
  kind: MarkKind;
  word: string;
  /** The daemon's reason, for a server that failed. */
  reason: string | null;
}

/** The shape, word and any reason for `info`'s state. */
export function serverState(info: McpServerInfo): ServerState {
  const { state, status } = info;
  const reason = typeof status === 'object' ? status.error : null;
  return { kind: SERVER_STATE_MARKS[state], word: TOOL_SERVER_WORDS[state], reason };
}
