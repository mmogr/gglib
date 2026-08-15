/**
 * Contract test: the body `POST /api/servers/start` receives, against the
 * Rust structs that deserialise it.
 *
 * This replaces a test that pinned a Tauri `serve_model` IPC command. There
 * is no such command — the Tauri surface is seven OS-integration commands
 * (`scripts/check-tauri-commands.sh` refuses any product command), and
 * `git log -S serve_model` finds it in no `.rs` file in this repository's
 * history. The old test also asserted a nested `{ id, request }` envelope
 * that nothing sends, against a literal it constructed itself, so it could
 * only ever confirm its own arithmetic.
 *
 * Two things are pinned here, both of which that test missed:
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
 *      `id` sits beside the config keys rather than wrapping them.
 *
 * Following this directory's rules: the extractor anchors on the
 * `rename_all` attribute rather than the struct name alone, and throws with
 * the symbol it could not find rather than returning a default.
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

// vitest runs with the project root as cwd, and the crates live beside it.
const TYPES_RS = readFileSync(
  resolve(process.cwd(), 'crates/gglib-app-services/src/types.rs'),
  'utf8',
);
const SERVERS_RS = readFileSync(
  resolve(process.cwd(), 'crates/gglib-axum/src/handlers/servers.rs'),
  'utf8',
);

const camel = (field: string) =>
  field.replace(/_([a-z])/g, (_, letter: string) => letter.toUpperCase());

/**
 * Wire field names of a `#[serde(rename_all = "camelCase")]` struct.
 *
 * Anchored on the attribute, not the name: a struct that loses `rename_all`
 * starts sending snake_case, and reading the fields without checking would
 * report the camelCase names it never sends again.
 */
function camelCaseWireFields(source: string, struct: string): string[] {
  const declaration = new RegExp(
    String.raw`#\[serde\(rename_all = "camelCase"\)\]\s*pub struct ${struct} \{([\s\S]*?)\n\}`,
  );
  const body = source.match(declaration)?.[1];
  if (!body) {
    throw new Error(
      `no #[serde(rename_all = "camelCase")] struct ${struct} — it was renamed, ` +
        'restructured, or lost the attribute, and this test no longer describes the wire',
    );
  }

  const fields = [...body.matchAll(/^\s{4}pub (\w+):/gm)].map((match) => camel(match[1]));
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

  it('names the model id `id`, the alias StartServerBody accepts', () => {
    expect(SERVERS_RS).toMatch(/#\[serde\(alias = "id"\)\]\s*pub model_id:/);
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

  it('omits id from the mapper output, which supplies it separately', () => {
    expect('id' in toStartServerRequest(FULL_CONFIG)).toBe(false);
  });

  it('defaults mlock to false rather than leaving it undefined', () => {
    // `mlock` is the one non-Option field on the Rust side, so an absent
    // value is a deserialisation error rather than a default.
    expect(toStartServerRequest({ id: 1 }).mlock).toBe(false);
  });
});
