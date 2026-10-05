/**
 * Contract test: the words the chat page reads a thinking model by, against
 * the Rust that writes them.
 *
 * The page offers its Thinking switch for a model by two strings it does not
 * own: the `reasoning` tag this machine's catalogue gives the model
 * (`capability_tags::REASONING`), and the `"reasoning"` entry a machine's
 * `/v1/models` lists for it (`REASONING_CAPABILITY` in `gglib-proxy`). Were
 * either renamed in Rust, nothing here would fail to compile: the switch
 * would stop showing, on every model, and say nothing about why.
 *
 * So each is read from the Rust source and compared, and so is the rule
 * between them: the daemon lists the capability by the tag, matched whatever
 * its case, which is the rule `thinks` follows for a local row.
 */

import { describe, it, expect } from 'vitest';

import { REASONING_CAPABILITY, REASONING_TAG, thinks } from '../../../src/utils/thinks';

import { fnSource, rust, withoutComments } from './rustSource';

const TAGS_RS = rust('crates/gglib-core/src/domain/capability_tags.rs');
const MODELS_LIST_RS = rust('crates/gglib-proxy/src/models_list.rs');

/**
 * The value of the one top-level `const NAME: &str = "…";` in `source`.
 *
 * Anchored at column zero and counted, so a same-named constant in a nested
 * module cannot stand in for it, and comments are removed first, so one
 * quoted in a doc comment cannot either. Throws, naming the constant, when
 * there is not exactly one.
 */
function strConst(source: string, name: string): string {
  const pattern = new RegExp(String.raw`^pub(?:\([^)]*\))? const ${name}: &str = "([^"]*)";`, 'gm');
  const found = [...withoutComments(source).matchAll(pattern)];
  if (found.length !== 1) {
    throw new Error(`expected exactly one top-level \`const ${name}: &str\`, found ${found.length}`);
  }
  return found[0][1];
}

describe('the words a thinking model is known by', () => {
  it("the page's tag is the catalogue's reasoning tag", () => {
    expect(REASONING_TAG).toBe(strConst(TAGS_RS, 'REASONING'));
  });

  it("the page's capability is the one /v1/models lists", () => {
    expect(REASONING_CAPABILITY).toBe(strConst(MODELS_LIST_RS, 'REASONING_CAPABILITY'));
  });

  it('the daemon lists the capability by the tag, and the page reads a local row by the same tag', () => {
    const listed = withoutComments(fnSource(MODELS_LIST_RS, 'capabilities_of'));
    expect(listed).toContain('capability_tags::is_reasoning(&summary.tags).then_some(REASONING_CAPABILITY)');
    expect(withoutComments(fnSource(TAGS_RS, 'is_reasoning'))).toContain('has(tags, REASONING)');

    const tag = strConst(TAGS_RS, 'REASONING');
    expect(thinks({ tags: [tag] })).toBe(true);
    expect(thinks({ capabilities: [strConst(MODELS_LIST_RS, 'REASONING_CAPABILITY')] })).toBe(true);
  });

  it('the daemon matches the tag whatever its case, and so does the page', () => {
    expect(withoutComments(fnSource(TAGS_RS, 'has'))).toContain('t.eq_ignore_ascii_case(tag)');
    const tag = strConst(TAGS_RS, 'REASONING');
    expect(thinks({ tags: [tag.toUpperCase()] })).toBe(true);
  });
});
