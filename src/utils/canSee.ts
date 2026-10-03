import type { GuiModel } from '../types/generated/GuiModel';
import type { ModelInfo } from '../types/generated/ModelInfo';

/** How a machine's `/v1/models` lists a model that reads images. */
const VISION_CAPABILITY = 'vision';

/**
 * A model as a row shows it: this machine's, which says `imageInput`, or the
 * paired machine's, which lists its `capabilities`.
 */
export type SeeingRow = Pick<GuiModel, 'imageInput'> | Pick<ModelInfo, 'capabilities'>;

/**
 * Whether the model on `row` reads images: it is linked to a projector on
 * the machine that has it. The one answer for both kinds of row, so a
 * surface that shows or gates on it asks here and not the field.
 */
export function canSee(row: SeeingRow): boolean {
  if ('imageInput' in row) return row.imageInput;
  return row.capabilities?.includes(VISION_CAPABILITY) ?? false;
}
