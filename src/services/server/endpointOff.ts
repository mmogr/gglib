/**
 * Why the endpoint is off, beside a load that left it off.
 *
 * A load whose endpoint could not come on, its port held by another
 * program, still loads the model (#2047): the daemon answers with the
 * load's success and `endpoint_off`, the endpoint status's own sentence.
 * Every place that loads says it the same way, as a warning, never as the
 * load's failure.
 *
 * @module services/server/endpointOff
 */

import type { ToastType } from '../../components/Toast';
import type { StartServerResponse } from '../../types/generated/StartServerResponse';

/** Say `answer`'s `endpoint_off` through `say`, when the load left the endpoint off. */
export function sayEndpointOff(answer: StartServerResponse, say: (message: string, type: ToastType) => void): void {
  if (answer.endpoint_off) say(answer.endpoint_off, 'warning');
}
