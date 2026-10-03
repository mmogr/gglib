/**
 * The paired machine's models, at `/api/remote/models*`: this machine's
 * daemon reads them through the tunnel with the key it holds, so the page
 * never does. What comes back is shown, never kept.
 *
 * A far model is named by its id there. It travels as one path segment, so
 * this side never parses or builds an identifier with a machine in it.
 */

import type { LoadResponse } from '../../../types/generated/LoadResponse';
import type { ModelLookup } from '../../../types/generated/ModelLookup';
import type { PairedModels } from '../../../types/generated/PairedModels';
import { REMOTE_MODELS_PATH } from '../../api/routes';
import { get, post } from './client';

function pairedModelPath(id: number): string {
  return `${REMOTE_MODELS_PATH}/${encodeURIComponent(String(id))}`;
}

/** Every model the paired machine lists, with that machine and what may be done there. */
export async function listPairedModels(): Promise<PairedModels> {
  return get<PairedModels>(REMOTE_MODELS_PATH);
}

/** One of the paired machine's models, by its id there. */
export async function getPairedModel(id: number): Promise<ModelLookup> {
  return get<ModelLookup>(pairedModelPath(id));
}

/** Have the paired machine load its model `id` now, at the context it would serve it with. */
export async function loadPairedModel(id: number): Promise<LoadResponse> {
  return post<LoadResponse>(`${pairedModelPath(id)}/load`, { num_ctx: null });
}
