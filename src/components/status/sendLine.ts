/**
 * The line above a chat's composer when a send has more to do than be
 * answered: "Qwen3-4B isn't loaded · sending loads it". The words are
 * `gglib_core::domain::send_line`'s, held to
 * `contracts/vocabulary/send_line.json` by
 * `tests/ts/contracts/sendLine.test.ts` for every absence and endpoint
 * state. Nothing locks: a send loads the model and turns the endpoint on,
 * and an endpoint pinned to another model refuses it, which the line says.
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

/** A model as the line knows it: its catalogue id, which a pin is matched by, and the name it shows. */
export interface NamedModel {
  id: number;
  name: string;
}

/**
 * The line for `model`, by the name the catalogue shows; `null` when the
 * model is resident and the endpoint is not off. `pinned` is the model a
 * pinned endpoint is pinned to: pinned to another (another id, whatever
 * its name), it refuses the send, whatever `model` is doing; a pin that
 * shares `model`'s name is said as "another model named …".
 */
export function sendLine(
  model: NamedModel,
  absence: ModelAbsence | null,
  endpoint: EndpointState,
  pinned: NamedModel | null,
): string | null {
  if (endpoint === 'pinned' && pinned !== null && pinned.id !== model.id) {
    return pinned.name === model.name
      ? `The endpoint is ${ENDPOINT_WORDS.pinned} to another model named ${pinned.name} and cannot serve this one · sending is refused`
      : `The endpoint is ${ENDPOINT_WORDS.pinned} to ${pinned.name} and cannot serve ${model.name} · sending is refused`;
  }
  const off = endpoint === 'off';
  if (absence === null) return off ? `The endpoint is ${ENDPOINT_WORDS.off} · sending turns it on` : null;
  const does = off ? `sending turns the endpoint on and loads it${AGAIN[absence]}` : `sending loads it${AGAIN[absence]}`;
  return `${model.name} ${WHAT[absence]} · ${does}`;
}
