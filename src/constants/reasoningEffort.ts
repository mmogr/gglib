/**
 * The reasoning-effort ladder, mirroring `gglib_core::domain::ReasoningEffort`.
 *
 * Six rungs, weakest first, spelled exactly as they go on the wire — the enum
 * serialises lowercase and llama.cpp never validates the string, so a typo here
 * renders into the prompt verbatim rather than being rejected (ADR 0007,
 * finding 5). That is why the option list is generated from this array and no
 * surface writes a level literal of its own.
 *
 * # There is no `none`
 *
 * Deliberately, and everywhere in gglib: erasing the kwarg is not a seventh
 * level, it is the absence of one, and what it yields is the *template's* own
 * default — measured as `medium` on gpt-oss. Offering "none" would promise the
 * user no thinking and deliver the template's preference instead. Clearing the
 * control (the empty option) is what omits the key, and the caption says so.
 *
 * # The two halves are not the same knob
 *
 * `reasoning_effort` is a *request* to the chat template. A template that does
 * not read the variable ignores it in silence, which is the whole reason
 * `reasoningEffortSupport` exists on the model detail. The budget below is
 * enforced by llama.cpp's own sampler, so it holds on every model regardless of
 * template — which is why the two are separate fields, and why only the effort
 * half is capability-gated in the UI.
 */

import type { ReasoningEffort } from '../types/generated/ReasoningEffort';

/**
 * Every level, weakest first. The order is the ladder's, not alphabetical, and
 * it is load-bearing: this array *is* the dropdown order.
 *
 * `satisfies` rather than a type annotation, so the literal tuple survives and
 * `ReasoningEffortLevel` stays a union of the six rather than widening to
 * `ReasoningEffort`. It catches a level invented here that Rust does not have.
 */
export const REASONING_EFFORT_LEVELS = [
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
] as const satisfies readonly ReasoningEffort[];

/** One rung of {@link REASONING_EFFORT_LEVELS}. */
export type ReasoningEffortLevel = (typeof REASONING_EFFORT_LEVELS)[number];

/**
 * The other direction: a rung Rust gained that this array has not.
 *
 * `satisfies` above cannot see that — an array of six valid levels satisfies
 * `readonly ReasoningEffort[]` whether or not a seventh exists. Without this,
 * a new Rust variant reaches the wire and the control silently never offers
 * it, which is the same shape of silence the model-event lane sat in.
 *
 * Written as a *constraint* rather than a conditional. `T extends Unlisted ?
 * never : T` over a naked parameter distributes, and distribution over `never`
 * yields `never`, so the conditional form passes whatever it is given.
 * Exported because `noUnusedLocals` is on and an unused type alias is not
 * exempt.
 */
type AssertNoUnlistedLevels<T extends never> = T;
export type ReasoningLadderIsComplete = AssertNoUnlistedLevels<
  Exclude<ReasoningEffort, ReasoningEffortLevel>
>;
