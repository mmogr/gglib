/**
 * The page's names for stored secrets (ADR 0027): the proxy key's reference
 * for the one reveal, and the command that prints each secret in a terminal,
 * for a sentence that points a person at it.
 *
 * Rust's `SettingSecret::reveal_command` is the commands' spelling, and
 * `tests/ts/contracts/revealCommands.test.ts` fails when the two disagree.
 *
 * @module constants/secrets
 */

import type { SecretRef } from '../types/generated/SecretRef';
import type { SettingSecret } from '../types/generated/SettingSecret';

/** The proxy's API key, as `revealSecret` names it. */
export const PROXY_API_KEY: SecretRef = { kind: 'setting', secret: 'proxy_api_key' };

export const REVEAL_COMMAND: Record<SettingSecret, string> = {
  proxy_api_key: 'gglib config settings reveal proxy-api-key',
  remote_pairing_key: 'gglib config settings reveal remote-pairing.api-key',
};
