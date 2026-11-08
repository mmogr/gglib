/**
 * The line above a chat's composer when a send has more to do than be
 * answered: "Qwen3-4B isn't loaded · sending loads it". The words are
 * `gglib_core::domain::send_line`'s, held to
 * `contracts/vocabulary/send_line.json` by
 * `tests/ts/contracts/sendLine.test.ts` for every absence and endpoint
 * state. Nothing locks: a send loads the model and turns the endpoint on.
 *
 * @module components/status/sendLine
 */

import { ENDPOINT_WORDS } from './words';
import type { EndpointState } from '../../types/generated/EndpointState';
import type { ModelAbsence } from '../../types/generated/ModelAbsence';

/** What is wrong, for each absence, after the model's name. */
const WHAT: Readonly<Record<ModelAbsence, string>> = {
  not_loaded: "isn't loaded",
  crashed: 'stopped unexpectedly',
};

/** What a send does about it, for each absence: loads it, or loads it again. */
const AGAIN: Readonly<Record<ModelAbsence, string>> = {
  not_loaded: '',
  crashed: ' again',
};

/**
 * The line for `model`, by the name the catalogue shows; `null` when the
 * model is resident and the endpoint is not off.
 */
export function sendLine(model: string, absence: ModelAbsence | null, endpoint: EndpointState): string | null {
  const off = endpoint === 'off';
  if (absence === null) return off ? `The endpoint is ${ENDPOINT_WORDS.off} · sending turns it on` : null;
  const does = off ? `sending turns the endpoint on and loads it${AGAIN[absence]}` : `sending loads it${AGAIN[absence]}`;
  return `${model} ${WHAT[absence]} · ${does}`;
}
