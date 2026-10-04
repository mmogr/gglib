import { useMemo } from 'react';
import type { ModelRef } from '../types/generated/ModelRef';
import { canSee } from '../utils/canSee';
import { usePairedModels } from './usePairedModels';

/** Whether the composer offers images, why not, and the context their cost is read against. */
export interface ImageInput {
  /** Whether an image may be attached: picked, pasted or dropped. */
  offered: boolean;
  /** Why not, as the attach button's tooltip says it; null when offered. */
  reason: string | null;
  /** The model's context, for "P% of context"; null when it is not known. */
  contextLength: number | null;
}

/** What the composer reads about the chat's model to decide. */
export interface ImageInputModel {
  /** A far chat: the far machine picks the model, and refuses an image by name. */
  far: boolean;
  /** The paired machine's model the chat is with, if it is. */
  paired?: ModelRef;
  /** This machine's model: whether it reads images (null = not known yet). */
  sees: boolean | null;
  /** This machine's model: its context, when known. */
  contextLength: number | null;
}

const CANNOT_SEE = 'This model cannot read images: it has no projector.';

/**
 * Whether the chat's model takes images, by one rule (`canSee`): this
 * machine's model by its catalogue entry, the paired machine's by the row
 * that machine lists for it, its rows read only for a chat on one of its
 * models. A far chat always offers them, since its model is chosen there; its
 * context is not known here, so its cost is tokens alone. A model not known
 * yet is offered: its proxy refuses by name. The answer is the same object
 * until one of its fields changes.
 */
export function useImageInput({ far, paired, sees, contextLength }: ImageInputModel): ImageInput {
  const { group } = usePairedModels(!far && paired !== undefined);
  // A far chat's: offered, its context not known here.
  let offered = true;
  let context: number | null = null;
  if (!far && paired) {
    const row = group?.models.find((m) => m.gglib_id === paired.id);
    offered = row ? canSee(row) : true;
    context = row?.context_window ?? null;
  } else if (!far) {
    offered = sees !== false;
    context = contextLength;
  }
  const reason = offered ? null : CANNOT_SEE;
  return useMemo(() => ({ offered, reason, contextLength: context }), [offered, reason, context]);
}
