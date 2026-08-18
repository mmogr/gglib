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

import { fnSource, rust } from './rustSource';

const HANDLERS = rust('crates/gglib-axum/src/handlers/mcp.rs');

/**
 * The `T` in a handler's `Result<Json<T>, HttpError>`, or `''` when it returns
 * no body.
 *
 * Anchored through `fnSource`, which resolves the name once in the file or
 * throws — this directory's rule, and not a decoration: six of the seven
 * handlers here are named `list`, `add`, `update`, `start`, `stop` and
 * `remove`, and a first-match `indexOf` over names that common is how a
 * contract test comes to describe the wrong function and report success.
 *
 * The return clause is taken after the signature's `) -> `, never as the first
 * `Json<…>` in the text: `add` and `update` accept a `Json<Request>`
 * extractor, so a leading match reads the body they *receive*.
 */
function jsonPayload(handler: string): string {
  const signature = fnSource(HANDLERS, handler).split(' {')[0];
  const arrow = signature.lastIndexOf(') -> ');
  if (arrow === -1) throw new Error(`fn ${handler} declares no return type`);

  const clause = signature.slice(arrow);
  const open = clause.indexOf('Json<');
  if (open === -1) return '';

  // Balanced, because `list` answers with `Json<Vec<McpServerInfo>>` and a
  // non-greedy `[^>]+` would report `Vec<McpServerInfo` — a string that
  // matches nothing anyone would write, so the assertion fails for the wrong
  // reason and the next reader "fixes" it by loosening it.
  let depth = 0;
  for (let i = open + 'Json'.length; i < clause.length; i += 1) {
    if (clause[i] === '<') depth += 1;
    else if (clause[i] === '>') {
      depth -= 1;
      if (depth === 0) return clause.slice(open + 'Json<'.length, i);
    }
  }
  throw new Error(`fn ${handler} has an unbalanced return type: ${clause}`);
}

describe('MCP route return shapes', () => {
  // Not `toContain`: the two the client got wrong were wrong by naming the
  // *inner* type, so an assertion that merely looks for `McpServerInfo`
  // somewhere in `Result<Json<McpServerDto>, …>` would have to see the whole
  // clause change to notice. Equality on the payload is what bites.
  it.each(['add', 'update', 'start', 'stop'])(
    '%s answers with the nested McpServerInfo',
    (handler) => {
      expect(jsonPayload(handler)).toBe('McpServerInfo');
    },
  );

  it('lists them as an array of the same nested shape', () => {
    expect(jsonPayload('list')).toBe('Vec<McpServerInfo>');
  });

  it('reads the return clause, not the request body the route accepts', () => {
    // `add` and `update` both take `Json<…Request>` as an argument. If the
    // extractor ever regresses to a leading match, these are what it reports.
    expect(jsonPayload('add')).not.toBe('CreateMcpServerRequest');
    expect(jsonPayload('update')).not.toBe('UpdateMcpServerRequest');
  });

  it('distinguishes a route that sends no body at all', () => {
    expect(jsonPayload('remove')).toBe('');
  });
});
