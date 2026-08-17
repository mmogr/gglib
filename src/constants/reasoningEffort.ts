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

/** Every level, weakest first. The order is the ladder's, not alphabetical. */
export const REASONING_EFFORT_LEVELS = [
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
] as const;

/** One rung of {@link REASONING_EFFORT_LEVELS}. */
export type ReasoningEffortLevel = (typeof REASONING_EFFORT_LEVELS)[number];
