import type { GuiModel } from '../types/generated/GuiModel';
import type { ModelInfo } from '../types/generated/ModelInfo';

/** The tag this machine's catalogue gives a model that thinks. */
export const REASONING_TAG = 'reasoning';

/** How a machine's `/v1/models` lists a model that thinks. */
export const REASONING_CAPABILITY = 'reasoning';

/**
 * A model as a row shows it: this machine's, which has its `tags`, or the
 * paired machine's, which lists its `capabilities`.
 */
export type ThinkingRow = Pick<GuiModel, 'tags'> | Pick<ModelInfo, 'capabilities'>;

/**
 * Whether the model on `row` thinks, which is what a Thinking switch is
 * offered for. A local row by its `reasoning` tag, in any case, as the daemon
 * matches it; never by its capability bit, which is read off the template and
 * launches nothing. A far row by the `reasoning` capability its machine
 * lists, which a gglib from before the switch never does.
 */
export function thinks(row: ThinkingRow): boolean {
  if ('tags' in row) return row.tags.some((tag) => tag.toLowerCase() === REASONING_TAG);
  return row.capabilities?.includes(REASONING_CAPABILITY) ?? false;
}
