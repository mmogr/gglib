/**
 * What the Load palette offers and what each row says: pure, so the rules
 * are pinned without a screen.
 *
 * This machine's rows are the library's models not already loaded, matching
 * the filter as the library's search matches (`matchesModel`). Each says its
 * fit as `GET /api/models/:id/explain` answers: the fit sentence the daemon
 * serves on `plan.fit` and, when the launch would unload a model first, the
 * unload sentence it serves on `plan.unloads`; an image model has no fit, so
 * only the unload sentence when there is one. A read that failed, or a daemon
 * without the route, is "fit unknown".
 *
 * The paired machine's rows are the library's far group (`usePairedModels`,
 * a machine joined), matching as the library matches them (`matchesFar`).
 * One loads there only when that machine lists Load for its models and its
 * rows are current, as the far inspector's Load allows; otherwise it says
 * why not.
 *
 * @module places/endpoint/paletteRows
 */

import type { PairedModelsState } from '../../hooks/usePairedModels';
import type { GgufModel, LaunchExplanation } from '../../types';
import type { ModelInfo } from '../../types/generated/ModelInfo';
import { formatSize } from '../../utils/format';
import { matchesFar, matchesModel } from '../library/groups';

/** One model's read: the explanation, `'failed'`, or `undefined` while it is still to come. */
export type FitRead = LaunchExplanation | 'failed' | undefined;

/** The words of a read that failed, or of a daemon that has no explain route. */
const FIT_UNKNOWN = 'fit unknown';

/** This machine's models the palette offers: not loaded, matching `query`, in the library's order. */
export function localChoices(models: readonly GgufModel[], loaded: ReadonlySet<number>, query: string): GgufModel[] {
  return models.filter((m) => !loaded.has(m.id) && matchesModel(m, query));
}

/** What a local row says beside its name: the quant and the size on disk. */
export const localAside = (model: GgufModel): string =>
  [model.quantization, model.sizeBytes > 0 && formatSize(model.sizeBytes)].filter(Boolean).join(' · ');

/**
 * A local row's second line, from its read: the unload first (the fact that
 * changes what loading does, kept clear of the ellipsis), then the fit;
 * `null` while reading or with nothing to say.
 */
export function fitLine(read: FitRead): string | null {
  if (read === undefined) return null;
  if (read === 'failed') return FIT_UNKNOWN;
  const unloads = read.plan.unloads;
  return [unloads?.sentence, read.plan.fit?.sentence].filter(Boolean).join(' ') || null;
}

/** The paired machine's rows, and why none of them loads there now (`null` when they do). */
export interface FarChoices {
  name: string;
  models: ModelInfo[];
  blocked: string | null;
}

/** The On <machine> group, or `null` with no machine joined. */
export function farChoices(paired: PairedModelsState, query: string): FarChoices | null {
  const group = paired.group;
  if (group?.machine.kind !== 'paired') return null;
  const blocked =
    paired.reach === 'away'
      ? `${paired.name} is away; Load waits until it is back.`
      : paired.reach === 'stale'
        ? `${paired.name}'s models could not be read just now.`
        : !group.actions.includes('load')
          ? `${paired.name} does not offer Load.`
          : null;
  return { name: paired.name, models: group.models.filter((m) => matchesFar(m, query)), blocked };
}
