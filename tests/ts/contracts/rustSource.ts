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
 * the next quote 34 lines below, blanking every field in between and hiding
 * four of them from the override scan entirely.
 */
export const scannable = (source: string): string =>
  withoutComments(source).replace(/"(?:[^"\\]|\\.)*"/g, '""');

/**
 * Rust with comment lines removed, strings left intact.
 *
 * For guards that need to read a string's *contents* — `rename_all =
 * "camelCase"` — where [`scannable`] would blank the very value being looked
 * for, while a doc comment quoting the attribute must still not stand in for
 * the real one.
 */
export const withoutComments = (source: string): string =>
  source.replace(/^[^\S\n]*\/\/.*$/gm, '');

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
      `expected exactly one top-level ${what}, found ${matches.length} — renamed, ` +
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
  // satisfied the guard for a struct that had lost it. Capturing only the
  // contiguous run of column-zero attribute and comment lines cannot.
  // Each attribute or comment line is bounded to one line. `[\s\S]*?` inside
  // `#[...]` looks lazier than it is: it will cross any number of newlines to
  // let the rest of the pattern match, and did — the capture ran from an
  // unrelated struct hundreds of lines above, dragging four other structs'
  // `rename_all` attributes in with it. A genuinely multi-line attribute is
  // therefore not captured, which fails loud (the guard throws) rather than
  // quiet (a neighbour's attribute standing in).
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
 * Exactly one top-level function's source, from its signature to the closing
 * brace at the given indent.
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
