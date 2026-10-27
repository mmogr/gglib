/**
 * The page's copy of each reveal command (`src/constants/secrets.ts`)
 * against the Rust that builds them, `SettingSecret::reveal_command`
 * (ADR 0027). A hint on the page that names a command the CLI does not take
 * sends a person nowhere, and nothing else would notice.
 */

import { describe, it, expect } from 'vitest';

import { REVEAL_COMMAND } from '../../../src/constants/secrets';

import { rust } from './rustSource';

const SECRET_RS = rust('crates/gglib-core/src/secret.rs');

/** The one match of `pattern` in `secret.rs`; throws, never defaults. */
function only(pattern: RegExp, what: string): RegExpMatchArray {
  const found = [...SECRET_RS.matchAll(pattern)];
  if (found.length !== 1) {
    throw new Error(`${what}: ${found.length} matches in crates/gglib-core/src/secret.rs`);
  }
  return found[0];
}

/** A variant's name as `rename_all = "snake_case"` writes it on the wire. */
const snakeCase = (variant: string): string =>
  variant.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();

describe('the page names each reveal as SettingSecret builds it', () => {
  it('has every secret, by its wire name, with the command reveal_command spells', () => {
    const prefix = only(/concat!\("([^"]+)", \$name\)/g, 'the command reveal_command builds')[1];
    const table = only(/^named! \{\n([\s\S]*?)^\}/gm, 'the named! table of names')[1];
    only(
      /#\[serde\(rename_all = "snake_case"\)\]\npub enum SettingSecret \{/g,
      "SettingSecret's snake_case wire names",
    );
    const rows = [...table.matchAll(/^\s+(\w+) => "([^"]+)",?$/gm)];
    expect(rows.length, 'no names in the named! table').toBeGreaterThan(0);

    const fromRust = Object.fromEntries(
      rows.map(([, variant, name]) => [snakeCase(variant), `${prefix}${name}`]),
    );
    expect(REVEAL_COMMAND).toEqual(fromRust);
  });
});
