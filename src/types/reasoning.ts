// ============================================================================
// Reasoning-control types
// ============================================================================
//
// The wire shapes that exist because a reasoning control is conditional in a
// way no sampling parameter is: `temperature` reaches the sampler on every
// model, while `reasoning_effort` reaches only a chat template that reads the
// variable — a fact gglib can measure but not assume (ADR 0007).
//
// Kept out of `./index` rather than added to it: these describe one mechanism
// and are read together, and `index.ts` is a hub that is over the file-size
// budget even after this migration cut it by a third. Re-exported from there
// so callers keep importing from `../types`. This file and `./index` have a
// type-only mutual import, which TypeScript permits and erases at compile
// time — the same arrangement `./benchmark` documents.
//
// @module types/reasoning

import type { SamplingParamKey } from './index';

/**
 * Whether a model's chat template reads a given kwarg, as llama-server
 * reported it the last time the model was served.
 *
 * Mirrors the backend `gglib_core::domain::Support`. Three states, never two:
 * the caps are read from `GET /props` while the model runs, so every model
 * nobody has launched on this installation answers `unknown` — and collapsing
 * that into `no` would hide the control on most of the library. The server's
 * own suppression acts only on `no` (ADR 0007 decision 3), so a surface offers
 * the control on `yes` and `unknown` alike and explains itself on `no`.
 */
// Aliased rather than renamed, to keep this commit free of consumer edits.
// `ProxyReasoningRows.tsx` declares a *component* named `TemplateSupport`;
// the two never meet, but a rename to `Support` at the call sites would be
// the tidier end state.
export type { Support as TemplateSupport } from './generated/Support';

/**
 * Every key the backend attributes a source to — the seventeen `wire_key`
 * (`gglib-app-services/src/sampling_explain.rs`) renders out of
 * `FieldSources::iter`, reasoning included.
 *
 * Seventeen and not `InferenceConfig`'s eighteen: `FieldSources` has no field
 * for `seed`, so no `sources` entry ever names it and the explain table has
 * no row for it. That is why this union is built up from `SamplingParamKey`
 * rather than derived from `keyof InferenceConfig`, which would admit a key
 * the endpoint cannot produce.
 *
 * A superset of `SamplingParamKey` rather than the same union, because the
 * explain table has to name a field the bounds table cannot describe: a
 * consumer indexing `resolved` with `reasoningEffort` gets a level, not a
 * quantity, and `INFERENCE_PARAMS` has no honest `{ min, max, step }` for it.
 */
export type ProvenanceParamKey = SamplingParamKey | 'reasoningEffort';

/**
 * A resolved `reasoningEffort` a model's template would not read — the
 * `effortSuppressed` field of `GET /api/models/:id/explain`.
 *
 * Conditional, not historical: the endpoint explains stored configuration, so
 * nothing has been sent. A surface must word it as what *would* happen on a
 * request against this model.
 */
