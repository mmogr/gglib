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
 *      `id` sits beside the config keys rather than wrapping them. Both are
 *      pinned, and they fail differently: without `flatten` the body will not
 *      deserialise at all (`missing field 'config'`), while without the alias
 *      it deserialises fine and the handler answers 400 on a missing model
 *      id. Nothing else here would notice either.
 *
 *   3. **No `inferenceParams` key the backend would silently drop.**
 *
 * What is deliberately *not* pinned: that the GUI sends every
 * `InferenceConfig` field. It sends 9 of 18 — `frequency_penalty`,
 * `dynatemp_*`, `top_n_sigma`, the four `dry_*` and `seed` are omitted — and
 * each is `Option`, so omitting them is well-formed.
 *
 * Nine of those omissions are *not* well-formed as UX, which is worth writing
 * down rather than leaving as an unexplained subset: the serve modal renders
 * `InferenceParametersForm`, which offers `frequency_penalty`, the `dynatemp_*`
 * pair, `top_n_sigma` and all four `dry_*` fields — controls a user can set and
 * this mapper then drops on the floor. Only `seed` has no control. That gap
 * predates the reasoning controls and is left as it is here; the two reasoning
 * fields were added to the mapper precisely so they would not join it.
 *
 * Note the asymmetry, because it sets what this guard is worth: nothing on
 * this path sets `deny_unknown_fields`, so an unknown key is *ignored*, not
 * rejected. The failure it catches is therefore a GUI control that silently
 * stops reaching the backend — no error, no 422, just a setting that stops
 * working.
 *
 * Following this directory's rules: the extractors anchor on the guard —
 * `rename_all` for a wire-name list, the struct body for an attribute — and
 * throw naming the symbol they could not find rather than returning a default.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

import { declaration, rust, scannable, structBody, withoutComments } from './rustSource';

import type { ServeConfig } from '../../../src/types';
import { toStartServerRequest } from '../../../src/services/transport/mappers';
import { MOCK_PROXY_PORT } from '../fixtures/ports';

const post = vi.fn();
vi.mock('../../../src/services/transport/api/client', () => ({
  post: (path: string, body?: unknown) => post(path, body),
  get: vi.fn(),
}));

const TYPES_RS = rust('crates/gglib-app-services/src/types.rs');
const SERVERS_RS = rust('crates/gglib-axum/src/handlers/servers.rs');
const INFERENCE_RS = rust('crates/gglib-core/src/domain/inference.rs');

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
  const { attrs, body } = declaration(source, struct);

  // Read from the attribute block belonging to *this* declaration, comments
  // stripped so a doc comment quoting the attribute cannot stand in for it.
  // `[^)]*` on both sides so a second option in the same attribute
  // (`deny_unknown_fields`) is not read as the attribute having been removed.
  if (!/#\[serde\([^)]*rename_all = "camelCase"[^)]*\)\]/.test(withoutComments(attrs))) {
    throw new Error(
      `no #[serde(rename_all = "camelCase")] on struct ${struct} — it was renamed, ` +
        'restructured, or lost the attribute, and this test no longer describes the wire',
    );
  }

  // A per-field `rename`/`skip`/`flatten` overrides the struct-level rule, so
  // reading names alone would report a wire field that is not on the wire.
  // Refuse rather than guess.
  //
  // Scanned with strings blanked: bounding on `)` missed serde's list form,
  // and bounding on `]` merely moved the blind spot to a `]` inside a string
  // literal, which `rename` and `alias` both accept. `cfg_attr` counts too — a
  // feature-gated rename changes the wire under exactly one build.
  //
  // `skip_serializing_if` is the one deliberate exemption: it affects presence
  // rather than name, and only when serialising, which is not the direction
  // this body travels. The `\b` is what excludes it, since `_` is a word
  // character.
  const override_ = scannable(body).match(
    /#\[(?:serde|cfg_attr)\([^\]]*(?:\b(?:rename|flatten)\b|\bskip(?:_serializing|_deserializing)?\b)[^\]]*\]/,
  );
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
  reasoningEffort: 'high',
  reasoningBudgetTokens: 4096,
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

    const sent = toStartServerRequest(FULL_CONFIG);
    expect(Object.keys(sent).sort()).toEqual(expected.sort());

    // Keys are not enough, and neither is "is it undefined". A key can carry
    // `undefined` (absent from the JSON body), `null` (present, and read as
    // `None` for every `Option` here — the user's value dropped just the
    // same), a constant, or the wrong source field, and a presence-only check
    // passes all four. Pin the values, sourced from the fixture so the
    // wiring itself is what is asserted.
    expect(sent).toEqual({
      contextLength: FULL_CONFIG.contextLength,
      port: FULL_CONFIG.port,
      mlock: FULL_CONFIG.mlock,
      jinja: FULL_CONFIG.jinja,
      // Omitted on purpose: the backend auto-detects it from model tags.
      reasoningFormat: undefined,
      mtpDraftNMax: FULL_CONFIG.specDraftNMax,
      mtpDraftPMin: FULL_CONFIG.specDraftPMin,
      inferenceParams: {
        temperature: FULL_CONFIG.temperature,
        topP: FULL_CONFIG.topP,
        topK: FULL_CONFIG.topK,
        maxTokens: FULL_CONFIG.maxTokens,
        repeatPenalty: FULL_CONFIG.repeatPenalty,
        presencePenalty: FULL_CONFIG.presencePenalty,
        minP: FULL_CONFIG.minP,
        // The serve modal offers both, so both have to arrive. The effort is
        // conditional on the model's template and the budget is not, but that
        // distinction belongs to the server — a surface that dropped either
        // one here would be deciding it silently, on the wire.
        reasoningEffort: FULL_CONFIG.reasoningEffort,
        reasoningBudgetTokens: FULL_CONFIG.reasoningBudgetTokens,
      },
    });
  });

  it('is read by a StartServerBody that aliases `id` and flattens the rest', () => {
    // Scoped to the struct rather than the file: `servers.rs` may grow another
    // body type, and an alias on that one would satisfy a bare pattern while
    // this contract quietly broke.
    const body = structBody(SERVERS_RS, 'StartServerBody');

    // Without the alias the body still deserialises; `start_body` then answers
    // 400 on a missing model id, so every start-server call fails at the
    // handler instead of the extractor.
    expect(body).toMatch(/#\[serde\(alias = "id"\)\]\s*pub model_id:/);

    // Without `flatten`, Rust demands a nested `config` object and the body
    // fails to deserialise — a break no other assertion here would see.
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

  it('sends no inferenceParams key InferenceConfig would silently drop', () => {
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

  // `ServeConfig` extends `SparseInferenceConfig`, so a field can be `null` as
  // well as absent. Rust reads the two identically — `InferenceConfig`'s
  // fields are bare `Option<T>`, which serde fills with `None` either way —
  // so the mapper must too. `!== undefined` read a null as a chosen value and
  // built an object of nothing but nulls, which is a different spelling of
  // the same request rather than a different request; the point is that the
  // client should not invent a distinction the wire does not have.
  it('treats an explicit null as unset, not as a value the user chose', () => {
    const cleared = toStartServerRequest({
      id: 1,
      temperature: null,
      topP: null,
      topK: null,
      maxTokens: null,
      repeatPenalty: null,
      presencePenalty: null,
      minP: null,
      reasoningEffort: null,
      reasoningBudgetTokens: null,
    });

    expect(cleared.inferenceParams).toBeUndefined();
  });

  it('still builds the object when one real value sits among the nulls', () => {
    const one = toStartServerRequest({ id: 1, temperature: null, topK: 40 });

    expect(one.inferenceParams?.topK).toBe(40);
  });

  // `ServeConfig` was a hand-kept subset of `InferenceConfig`, and the mapper
  // was a second hand-kept list on top of it. Both named nine of eighteen, so
  // the other nine could be typed into a form, accepted by the endpoint, and
  // discarded in between with nothing to say so.
  //
  // Asserted against the *Rust struct*, not against `INFERENCE_CONFIG_KEYS`.
  // The mapper iterates that list, so comparing its output to it is a
  // tautology — both sides move together and the test passes on any subset.
  // What the wire accepts is the independent fact, and it is the one worth
  // pinning. (The list's own completeness is a compile-time assertion,
  // `InferenceConfigKeysAreComplete`, which is a stronger guard than a test.)
  it('forwards every InferenceConfig field the wire accepts', () => {
    const accepted = camelCaseWireFields(INFERENCE_RS, 'InferenceConfig');
    expect(accepted).toContain('dryBase'); // extractor sanity check

    const everything = Object.fromEntries(
      accepted.map((key) => [key, key === 'reasoningEffort' ? 'high' : 1]),
    ) as unknown as ServeConfig;

    const sent = toStartServerRequest({ ...everything, id: 1 }).inferenceParams ?? {};

    expect(Object.keys(sent).sort()).toEqual([...accepted].sort());
  });

  // The nine named, so a regression reads as a list rather than a count.
  it.each([
    'frequencyPenalty',
    'dynatempRange',
    'dynatempExponent',
    'topNSigma',
    'dryMultiplier',
    'dryBase',
    'dryAllowedLength',
    'dryPenaltyLastN',
    'seed',
  ])('no longer drops %s on the way to the launch', (field) => {
    const sent = toStartServerRequest({ id: 1, [field]: 1 }).inferenceParams;

    expect(sent).toEqual({ [field]: 1 });
  });
});
