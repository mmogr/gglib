/**
 * What a model's launch plan says of the model a launch would unload first
 * (`plan.unloads`), as a sentence, and a context size as the inspector
 * writes it. The fit sentence is not here: the daemon serves it on
 * `plan.fit.sentence`, the words `gglib model explain` prints.
 *
 * The unload sentence is the one `gglib model explain` prints
 * (`unload_sentence` in `gglib-cli`'s launch display), held to
 * `contracts/models/unload_reason.json` by
 * `tests/ts/contracts/unloadReason.test.ts` for every reason.
 *
 * @module readings/launchFit
 */

import type { Unload } from '../types/generated/Unload';
import type { UnloadReason } from '../types/generated/UnloadReason';

/** A context size: `32K` when it is whole thousands of 1,024, else the number. */
export const tokens = (n: number): string => (n % 1024 === 0 ? `${n / 1024}K` : n.toLocaleString('en-US'));

/** Why a launch unloads a model first, for each reason, as the CLI says it. */
export const UNLOAD_BECAUSE: Readonly<Record<UnloadReason, string>> = {
  would_not_fit_beside: 'it would not fit beside it',
  second_slot_full: 'the second slot is taken',
  one_image_model: 'one image model is loaded at a time',
};

/** The unload sentence: "Unloads Qwen3-30B first: it would not fit beside it." */
export const unloadSentence = (unload: Unload): string => `Unloads ${unload.model} first: ${UNLOAD_BECAUSE[unload.reason]}.`;
