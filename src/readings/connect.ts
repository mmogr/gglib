/**
 * What the Endpoint page says of a proof and of a client's recipes: the
 * words `gglib proxy test` and `gglib up` print, without the terminal's
 * mark and columns.
 *
 * `render_proof` and `render_client_config` in `gglib-cli`'s
 * `handlers/up/client_block.rs` are where these are written; their tests
 * record inputs beside the CLI's lines in `contracts/proxy/`, and
 * `tests/ts/contracts/connectClient.test.ts` holds these readers to them.
 * The recipes themselves are the daemon's (`GET /api/proxy/clients`); the
 * one thing decided here, as there, is which lines are key lines, so a key
 * the endpoint checks is said as the stored secret, never printed.
 *
 * @module readings/connect
 */

import { REVEAL_COMMAND } from '../constants/secrets';
import type { ClientRecipe } from '../types/generated/ClientRecipe';
import type { ClientRecipes } from '../types/generated/ClientRecipes';
import type { ProofOutcome } from '../types/generated/ProofOutcome';

/**
 * The proof's sentence when the endpoint answered: who answered and in how
 * long, to the nearest tenth of a second, and what was asked for when the
 * answer names another model.
 */
export function proofAnswered(proof: ProofOutcome): string {
  const tenths = Math.floor((proof.elapsed_ms + 50) / 100);
  const asked = proof.answered_by === proof.asked_for ? '' : ` (asked for ${proof.asked_for})`;
  return `answered in ${Math.floor(tenths / 10)}.${tenths % 10}s by ${proof.answered_by}${asked}`;
}

/** The proof's sentence when it did not complete, with the daemon's reason. */
export function proofFailed(reason: string): string {
  return `the first request did not complete: ${reason}`;
}

/** How a stored key is said where it would otherwise be printed (`held_line`). */
export function heldKeyLine(): string {
  return `(set, not shown)  \`${REVEAL_COMMAND.proxy_api_key}\` prints it`;
}

/**
 * The daemon's recipes as they are shown: in its order with its labels,
 * and every key line said as the stored secret when the endpoint checks
 * the key, in place of the daemon's placeholder.
 */
export function recipesShown(answer: ClientRecipes): ClientRecipe[] {
  const required = answer.key === 'required';
  return answer.recipes.map((recipe) => ({
    ...recipe,
    lines: recipe.lines.map((line) =>
      required && line.label.toLowerCase().includes('key') ? { ...line, value: heldKeyLine() } : line,
    ),
  }));
}
