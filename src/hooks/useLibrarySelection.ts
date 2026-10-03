/**
 * The library's one selection: a model of this machine's by its id, or one
 * of the paired machine's by its ref, never both.
 *
 * Picking either clears the other. A far pick also clears the local id
 * through `selectModel`, which tells the native menu (`set_selected_model`)
 * that nothing of this machine's is selected, so its Start, Stop and Remove
 * cannot act on a local model that happens to share the far one's number.
 *
 * A far pick holds only while the paired rows are that machine's: when they
 * are cleared, or are another machine's, it is dropped, not kept for later.
 *
 * @module useLibrarySelection
 */

import { useCallback, useEffect, useState } from 'react';
import type { ModelRef } from '../types/generated/ModelRef';
import type { PairedGroup } from './usePairedModels';

export interface LibrarySelection {
  /** The paired machine's model picked, if one is. */
  farPick: ModelRef | null;
  /** Pick one of the paired machine's models. */
  pickFar: (model: ModelRef) => void;
  /** Pick a model of this machine's, or nothing. */
  pickLocal: (id: number | null) => void;
}

function sameMachine(group: PairedGroup | null, model: ModelRef): boolean {
  const machine = group?.machine;
  return (
    machine?.kind === 'paired' &&
    model.machine.kind === 'paired' &&
    machine.fingerprint === model.machine.fingerprint
  );
}

export function useLibrarySelection(
  selectModel: (id: number | null) => void,
  group: PairedGroup | null,
): LibrarySelection {
  const [picked, setPicked] = useState<ModelRef | null>(null);
  const farPick = picked !== null && sameMachine(group, picked) ? picked : null;

  // Dropped rather than hidden, so rows coming back do not bring it with them.
  useEffect(() => {
    if (picked !== null && farPick === null) setPicked(null);
  }, [picked, farPick]);

  const pickFar = useCallback(
    (model: ModelRef) => {
      setPicked(model);
      selectModel(null);
    },
    [selectModel],
  );

  const pickLocal = useCallback(
    (id: number | null) => {
      setPicked(null);
      selectModel(id);
    },
    [selectModel],
  );

  return { farPick, pickFar, pickLocal };
}
