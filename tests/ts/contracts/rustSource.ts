/**
 * Reading Rust from a TypeScript test, with the anchoring this directory's
 * README states as its rule: *anchor on the guard, not the name*, and *throw,
 * never default*.
 *
 * These live in one module because keeping them per-file did not work. Three
 * separate rounds of review found the same defeat — a same-named declaration
 * earlier in the file, `#[cfg(any())]`-gated or nested in a `mod`, satisfying
 * a guard the real declaration had lost — and each round's fix reached one
 * extractor and not its neighbours. An unanchored `indexOf('fn name(')` takes
 * the first match and cannot tell the two apart, so the drift it exists to
 * catch reports success.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

/**
 * Repo root, relative to this file rather than the cwd.
 *
 * These paths would otherwise resolve against wherever vitest was invoked
 * from, so running it anywhere but the repo root — an IDE runner, or `--root`
 * pointed back here from a subdirectory — turns a contract test into an
 * ENOENT rather than a failure it can explain.
 */
export const REPO_ROOT = resolve(import.meta.dirname, '../../..');

/** Read a Rust source file, by path from the repo root. */
export const rust = (path: string): string => readFileSync(resolve(REPO_ROOT, path), 'utf8');

/**
 * Rust with comments removed and every string literal blanked, so a delimiter
 * inside either cannot terminate an attribute pattern early.
 *
 * Order matters, and getting it wrong is not theoretical: blanking strings
 * first pairs the lone `"` in `InferenceConfig`'s DRY doc comment — which
 * documents llama.cpp's sequence breakers as `` `\n`, `:`, `"`, `*` `` — with
 * the next quote 34 lines below, blanking every field in between. Four are
 * hidden by that span directly, and the parity error then cascades into the
 * next one, taking `seed` with it — five in total.
 */
export const scannable = (source: string): string =>
  withoutComments(source).replace(/"(?:[^"\\]|\\.)*"/g, '""');

/**
 * Rust with comments removed, strings left intact.
 *
 * For guards that need to read a string's *contents* — `rename_all =
 * "camelCase"` — where [`scannable`] would blank the very value being looked
 * for, while a comment quoting the attribute must still not stand in for the
 * real one.
 *
 * Scanned rather than regexed, because the regex form only removed *whole-line*
 * comments and the hole that left was the one the blanking exists to close: a
 * single unbalanced quote in a trailing `// comment "` pairs with the quote in
 * the attribute below it, blanking `#[serde(rename = ` out of existence. A
 * live per-field rename then passed the guard that exists to catch it.
 *
 * Not a Rust lexer: raw strings (`r#"…"#`) and byte strings are not modelled.
 * Neither appears in the declarations these tests read, and the failure
 * direction is a spurious throw naming the struct, not a silent pass.
 */
export function withoutComments(source: string): string {
  let out = '';
  let i = 0;

  while (i < source.length) {
    const pair = source.slice(i, i + 2);

    if (pair === '//') {
      while (i < source.length && source[i] !== '\n') i += 1;
      continue;
    }

    if (pair === '/*') {
      i += 2;
      while (i < source.length && source.slice(i, i + 2) !== '*/') i += 1;
      i += 2;
      continue;
    }

    // Copy string literals through untouched, so a `//` or `/*` inside one is
    // not mistaken for a comment.
    if (source[i] === '"') {
      out += source[i];
      i += 1;
      while (i < source.length) {
        if (source[i] === '\\') {
          out += source.slice(i, i + 2);
          i += 2;
          continue;
        }
        out += source[i];
        i += 1;
        if (source[i - 1] === '"') break;
      }
      continue;
    }

    out += source[i];
    i += 1;
  }

  return out;
}

/**
 * The one and only match of `pattern`, or a throw naming what went wrong.
 *
 * Uniqueness is the point: "first match wins" is what lets a decoy stand in
 * for the real declaration.
 */
function only(source: string, pattern: RegExp, what: string): RegExpMatchArray {
  const matches = [...source.matchAll(pattern)];
  if (matches.length !== 1) {
    throw new Error(
      `expected exactly one ${what}, found ${matches.length} — renamed, ` +
        'restructured, or shadowed by a same-named declaration',
    );
  }
  return matches[0];
}

/**
 * Exactly one top-level struct: its attribute block and its body.
 *
 * `attrs` is the contiguous run of attributes and doc comments directly above
 * the declaration, so a guard read from it belongs to *this* struct.
 */
export function declaration(source: string, struct: string): { attrs: string; body: string } {
  // The attribute block is captured by the match, not sliced back to the
  // previous blank line. A blank line is a formatting convention — rustfmt
  // neither requires nor inserts one between items — so slicing reached back
  // over a whole preceding declaration, and a neighbour's `rename_all` then
  // satisfied the guard for a struct that had lost it.
  //
  // Attribute and comment lines are matched one at a time, also deliberately.
  // An earlier draft allowed `[\s\S]*?` between `#[` and `]`, which is far
  // less lazy than it looks — it crosses as many newlines as the rest of the
  // pattern needs, and did, capturing from an unrelated struct hundreds of
  // lines above and dragging four other structs' `rename_all` in with it.
  //
  // The cost is that a multi-line attribute (rustfmt produces them past the
  // width limit) is not captured at all: `attrs` comes back empty. Callers
  // that read it therefore throw — the safe direction — but `structBody` does
  // not read it, so for that path a multi-line attribute is simply invisible.
  const match = only(
    source,
    new RegExp(
      String.raw`^((?:(?:#\[[^\n]*\]|\/\/[^\n]*)\n)*)pub(?:\([^)]*\))? struct ${struct} \{([\s\S]*?)\n\}`,
      'gm',
    ),
    `struct ${struct}`,
  );

  return { attrs: match[1], body: match[2] };
}

/** Exactly one top-level struct's body. */
export const structBody = (source: string, struct: string): string =>
  declaration(source, struct).body;

/**
 * Exactly one function's source, from its signature to the closing brace at
 * the given indent.
 *
 * Unlike [`declaration`] this is not anchored at column zero, and must not be:
 * three of its callers want an `impl` method. Its guarantee is uniqueness —
 * the name resolves once in the file, or it throws.
 *
 * @param closeAt - the dedent that ends the body: `'\n}'` for a free function,
 *   `'\n    }'` for one inside an `impl`.
 */
export function fnSource(source: string, fnName: string, closeAt = '\n}'): string {
  const match = only(
    source,
    new RegExp(String.raw`^\s*(?:pub(?:\([^)]*\))? )?(?:const |async )?fn ${fnName}\(`, 'gm'),
    `fn ${fnName}`,
  );

  const start = match.index;
  const end = source.indexOf(closeAt, start);
  return source.slice(start, end === -1 ? undefined : end);
}
