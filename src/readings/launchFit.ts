/**
 * What a model's launch plan says of its fit, as a sentence: the context it
 * would be served at against what fits this machine (`plan.fit`), and the
 * model a launch would unload first (`plan.unloads`). The inspector says the
 * first under Load; the endpoint's Load palette says both on each row.
 *
 * The fit sentence is composed from `ContextFit`'s figures, nothing worked
 * out that the daemon did not say. The unload sentence is the one
 * `gglib model explain` prints (`unload_sentence` in `gglib-cli`'s
 * launch display), held to `contracts/models/unload_reason.json` by
 * `tests/ts/contracts/unloadReason.test.ts` for every reason.
 *
 * @module readings/launchFit
 */

import type { ContextFit } from '../types/generated/ContextFit';
import type { Unload } from '../types/generated/Unload';
import type { UnloadReason } from '../types/generated/UnloadReason';

/** A context size: `32K` when it is whole thousands of 1,024, else the number. */
export const tokens = (n: number): string => (n % 1024 === 0 ? `${n / 1024}K` : n.toLocaleString('en-US'));

/** What a refused fit was missing, from the figures that are `null`. */
function missing(fit: ContextFit): string {
  const gaps = [
    fit.budgetBytes === null && "this Mac's memory could not be read",
    fit.weightsBytes === null && "the weights' size is not known",
    fit.kvBytesPerToken === null && 'the model does not say what its context costs',
  ].filter(Boolean);
  return gaps.length > 0 ? `: ${gaps.join(', ')}` : '';
}

/**
 * The fit sentence: the context the model would be served at, of the window
 * it was trained for, and whether it fits this machine. `null` for an image
 * model, which has no context.
 */
export function fitSentence(fit: ContextFit | null): string | null {
  if (!fit) return null;
  const trained = fit.trainedCtx ? `, of ${tokens(fit.trainedCtx)} trained` : '';
  const at = tokens(fit.servesAt);
  if (fit.fitted === null) return `Serves at ${at} context${trained}; gglib could not fit one to this Mac${missing(fit)}.`;
  if (fit.servesAt <= fit.fitted) return `Fits this Mac with ${at} context${trained}.`;
  return `Serves at ${at} context${trained}; ${tokens(fit.fitted)} fits this Mac.`;
}

/** Why a launch unloads a model first, for each reason, as the CLI says it. */
export const UNLOAD_BECAUSE: Readonly<Record<UnloadReason, string>> = {
  would_not_fit_beside: 'it would not fit beside it',
  second_slot_full: 'the second slot is taken',
  one_image_model: 'one image model is loaded at a time',
};

/** The unload sentence: "Unloads Qwen3-30B first: it would not fit beside it." */
export const unloadSentence = (unload: Unload): string => `Unloads ${unload.model} first: ${UNLOAD_BECAUSE[unload.reason]}.`;
