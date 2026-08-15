/**
 * Contract test: the body `POST /api/servers/start` receives, against the
 * Rust structs that deserialise it.
 *
 * This replaces a test that pinned a Tauri `serve_model` IPC command. There
 * is no such command — the Tauri surface is seven commands, allowlisted by
 * name in `scripts/check-frontend-ipc.sh`, and `git log -S serve_model` finds
 * it in no `.rs` file in this repository's history. The old test asserted a
 * nested `{ id, request }` envelope that nothing sends, and asserted it
 * against a literal it constructed two lines earlier, so it could not have
 * caught drift on the Rust side at all.
 *
 * Three things are pinned here, all of which that test missed:
 *
 *   1. **Every field, not the convenient ones.** `toStartServerRequest`
 *      returns eight keys; the old assertions named five. Vitest's `toEqual`
 *      ignores `undefined` properties, so the three it omitted —
 *      `mtpDraftNMax`, `mtpDraftPMin`, `inferenceParams` — passed simply by
 *      never being set in a fixture. The field list is read from the Rust
 *      instead, so a field added there fails this test rather than silently
 *      never being sent.
 *
 *   2. **The body is flat.** `StartServerBody` takes the model id as
 *      `#[serde(alias = "id")]` and the rest through `#[serde(flatten)]`, so
 *      `id` sits beside the config keys rather than wrapping them. Both
 *      attributes are pinned: dropping either makes every start-server call
 *      fail deserialisation, and nothing else here would notice.
 *
 *   3. **No `inferenceParams` key the backend would reject.**
 *
 * What is deliberately *not* pinned: that the GUI sends every
 * `InferenceConfig` field. It sends 7 of 16 — `frequency_penalty`,
 * `dynatemp_*`, `top_n_sigma`, the four `dry_*` and `seed` have no GUI
 * control — and each is `Option`, so omitting them is well-formed. Only the
 * direction that breaks something is guarded: a key Rust would not accept.
 *
 * Following this directory's rules: the extractors anchor on the guard —
 * `rename_all` for a wire-name list, the struct body for an attribute — and
 * throw naming the symbol they could not find rather than returning a default.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { ServeConfig } from '../../../src/types';
import { toStartServerRequest } from '../../../src/services/transport/mappers';
import { MOCK_PROXY_PORT } from '../fixtures/ports';

const post = vi.fn();
vi.mock('../../../src/services/transport/api/client', () => ({
  post: (path: string, body?: unknown) => post(path, body),
  get: vi.fn(),
}));

// Relative to this file, not to the cwd: `vitest --root` moves the cwd and
// would turn a contract test into an ENOENT.
const REPO_ROOT = resolve(import.meta.dirname, '../../..');
const rust = (path: string) => readFileSync(resolve(REPO_ROOT, path), 'utf8');

const TYPES_RS = rust('crates/gglib-app-services/src/types.rs');
const SERVERS_RS = rust('crates/gglib-axum/src/handlers/servers.rs');
const INFERENCE_RS = rust('crates/gglib-core/src/domain/inference.rs');

const camel = (field: string) =>
  field.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase());

/** A named struct's body, so a pattern cannot match the wrong declaration. */
function structBody(source: string, struct: string): string {
  const body = source.match(
    new RegExp(String.raw`pub(?:\([^)]*\))? struct ${struct} \{([\s\S]*?)\n\}`),
  )?.[1];
  if (!body) {
    throw new Error(`no struct ${struct} — renamed or restructured`);
  }
  return body;
}

/**
 * Wire field names of a `#[serde(rename_all = "camelCase")]` struct.
 *
 * Anchored on the attribute, not the name: a struct that loses `rename_all`
 * starts sending snake_case, and reading the fields without checking would
 * report the camelCase names it never sends again.
 */
function camelCaseWireFields(source: string, struct: string): string[] {
  // Anchored at line start, with only whole attribute lines allowed between
  // the attribute and the struct. A bare `\s*` would let a doc comment
  // mentioning `#[serde(rename_all = "camelCase")]` stand in for the real one.
  // `[^)]*` on both sides so a second option in the same attribute
  // (`deny_unknown_fields`) is not read as the attribute having been removed.
  const declaration = new RegExp(
    String.raw`^#\[serde\([^)]*rename_all = "camelCase"[^)]*\)\]\n(?:#\[[^\n]*\]\n)*pub struct ${struct} \{([\s\S]*?)\n\}`,
    'm',
  );
  const body = source.match(declaration)?.[1];
  if (!body) {
    throw new Error(
      `no #[serde(rename_all = "camelCase")] struct ${struct} — it was renamed, ` +
        'restructured, or lost the attribute, and this test no longer describes the wire',
    );
  }

  // A per-field `rename`/`skip`/`flatten` overrides the struct-level rule, so
  // reading names alone would report a wire field that is not on the wire.
  // Refuse rather than guess. (`skip_serializing_if` has no word boundary
  // after "skip" and is deliberately not caught: it affects presence, not name,
  // and only in the response direction.)
  const override_ = body.match(/#\[serde\([^)]*\b(?:rename|skip|flatten)\b[^)]*\)\]/);
  if (override_) {
    throw new Error(
      `${struct} carries a per-field serde override (${override_[0]}) that this ` +
        'extractor cannot model — teach it the rule or pin the field explicitly',
    );
  }

  const fields = [...body.matchAll(/^\s{4}pub (?:r#)?(\w+):/gm)].map((match) => camel(match[1]));
  if (fields.length === 0) {
    throw new Error(`struct ${struct} parsed but yielded no fields`);
  }
  return fields;
}

/** A config that sets every optional the mapper knows how to forward. */
const FULL_CONFIG: ServeConfig = {
  id: 123,
  contextLength: 4096,
  port: MOCK_PROXY_PORT,
  mlock: true,
  jinja: true,
  specDraftNMax: 4,
  specDraftPMin: 0.75,
  temperature: 0.7,
  topP: 0.95,
  topK: 40,
  maxTokens: 512,
  repeatPenalty: 1.1,
  presencePenalty: 0,
  minP: 0.05,
};

describe('POST /api/servers/start request body', () => {
  beforeEach(() => {
    post.mockReset();
    post.mockResolvedValue({});
  });

  it('sends every field StartServerRequest declares, under its wire name', () => {
    const expected = camelCaseWireFields(TYPES_RS, 'StartServerRequest');

    // Sanity-check the extractor against a field that must be there: a
    // regex that matched nothing would otherwise make this test vacuous.
    expect(expected).toContain('contextLength');

    expect(Object.keys(toStartServerRequest(FULL_CONFIG)).sort()).toEqual(expected.sort());
  });

  it('is read by a StartServerBody that aliases `id` and flattens the rest', () => {
    // Scoped to the struct rather than the file: `servers.rs` may grow another
    // body type, and an alias on that one would satisfy a bare pattern while
    // this contract quietly broke.
    const body = structBody(SERVERS_RS, 'StartServerBody');

    expect(body).toMatch(/#\[serde\(alias = "id"\)\]\s*pub model_id:/);

    // Without `flatten`, Rust demands a nested `config` object and every
    // start-server call 422s — a break no other assertion here would see.
    expect(body).toMatch(/#\[serde\(flatten\)\]\s*pub config: StartServerRequest/);
  });

  it('flattens the config beside the id rather than nesting it', async () => {
    const { serveModel } = await import('../../../src/services/transport/api/servers');
    await serveModel(FULL_CONFIG);

    const [path, body] = post.mock.calls[0] as [string, Record<string, unknown>];
    expect(path).toBe('/api/servers/start');

    // `request` would be the nested shape the retired test asserted.
    expect(body).not.toHaveProperty('request');
    expect(body.id).toBe(123);
    expect(body.contextLength).toBe(4096);
    expect(body.inferenceParams).toMatchObject({ temperature: 0.7 });
  });

  it('sends no inferenceParams key InferenceConfig would reject', () => {
    const accepted = camelCaseWireFields(INFERENCE_RS, 'InferenceConfig');
    expect(accepted).toContain('topP'); // extractor sanity check

    const sent = Object.keys(toStartServerRequest(FULL_CONFIG).inferenceParams ?? {});
    expect(sent.length).toBeGreaterThan(0);
    expect(sent.filter((key) => !accepted.includes(key))).toEqual([]);
  });

  it('omits id from the mapper output, which supplies it separately', () => {
    expect('id' in toStartServerRequest(FULL_CONFIG)).toBe(false);
  });

  it('defaults mlock to false rather than leaving it undefined', () => {
    // `mlock` is the one non-Option field on the Rust side, so an absent
    // value is a deserialisation error rather than a default.
    expect(toStartServerRequest({ id: 1 }).mlock).toBe(false);
  });
});
