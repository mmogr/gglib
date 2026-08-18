/**
 * What the MCP server routes actually hand back.
 *
 * Every one of them returns the *nested* `McpServerInfo` — `{server, status,
 * tools}` — and the TypeScript client claimed a bare server row for two of
 * them. That is the same defect PR 1 fixed on `start` and `stop`, left behind
 * on `add` and `update` because their one caller discards the result: nothing
 * misbehaves at runtime, and `useMcpServers` still publicly promises a shape
 * the endpoint has never sent.
 *
 * Read off the handler signatures rather than asserted from memory, so a
 * route that changes what it returns fails here instead of drifting quietly.
 */

import { describe, it, expect } from 'vitest';

import { rust, withoutComments } from './rustSource';

const HANDLERS = withoutComments(rust('crates/gglib-axum/src/handlers/mcp.rs'));

/**
 * Whatever a handler is declared to return — the text between `) -> ` and the
 * opening brace.
 *
 * Deliberately not the first `Json<…>` in the signature: several of these take
 * a `Json<Request>` extractor as an argument, so a naive match reads the body
 * they accept rather than the body they send.
 */
function returnType(fn: string): string {
  const start = HANDLERS.indexOf(`async fn ${fn}(`);
  if (start < 0) throw new Error(`no handler named ${fn}`);
  const arrow = HANDLERS.indexOf(') -> ', start);
  const brace = HANDLERS.indexOf(' {', arrow);
  return HANDLERS.slice(arrow + ') -> '.length, brace).trim();
}

describe('MCP route return shapes', () => {
  it.each(['list', 'add', 'update', 'start', 'stop'])(
    '%s answers with the nested McpServerInfo',
    (handler) => {
      expect(returnType(handler)).toContain('McpServerInfo');
    },
  );

  // The two the client got wrong. Named separately so a regression reads as
  // "add and update again" rather than as a count.
  it.each(['add', 'update'])('%s does not answer with a bare server row', (handler) => {
    expect(returnType(handler)).not.toBe('McpServerDto');
  });

  it('reads the file it claims to — sanity check on the extractor', () => {
    expect(HANDLERS).toContain('pub(crate) async fn add(');
    expect(returnType('remove')).not.toContain('McpServerInfo');
  });
});
