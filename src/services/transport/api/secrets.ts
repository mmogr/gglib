/**
 * Secrets API module: the one reveal of a stored secret (ADR 0027).
 *
 * Every other read says only whether a secret is held. This is what the
 * page's Show and Copy ask, each time they are used.
 */

import { post } from './client';
import type { RevealedSecret } from '../../../types/generated/RevealedSecret';
import type { SecretRef } from '../../../types/generated/SecretRef';

/**
 * The stored secret `secret` names, or `null` when nothing is stored there
 * (`POST /api/secrets/reveal`). The pairing key is refused: only the CLI
 * prints it.
 */
export async function revealSecret(secret: SecretRef): Promise<string | null> {
  const answer = await post<RevealedSecret>('/api/secrets/reveal', secret, { sensitive: true });
  return answer.value;
}
