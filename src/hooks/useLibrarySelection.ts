/**
 * The library's one selection: a model of this machine's by its id, or one
 * of the paired machine's by its ref, never both.
 *
 * Picking either clears the other. The local pick is also the native menu's
 * (`set_selected_model`), so its Start, Stop and Remove act on the model the
 * page shows; a far pick tells it nothing of this machine's is selected, so
 * those cannot act on a local model that happens to share the far one's
 * number.
 *
 * A far pick holds only while the paired rows are that machine's: when they
 * are cleared, or are another machine's, it is dropped, not kept for later.
 *
 * @module useLibrarySelection
 */

import { useCallback, useEffect, useState } from 'react';
// TRANSPORT_EXCEPTION: setSelectedModel is desktop-only (menu sync)
import { appLogger, setSelectedModel } from '../services/platform';
import type { ModelRef } from '../types/generated/ModelRef';
import type { PairedGroup } from './usePairedModels';

export interface LibrarySelection {
  /** The model of this machine's picked, by id, if one is. */
  localPick: number | null;
  /** The paired machine's model picked, if one is. */
  farPick: ModelRef | null;
  /** Pick one of the paired machine's models. */
  pickFar: (model: ModelRef) => void;
  /** Pick a model of this machine's, or nothing. */
  pickLocal: (id: number | null) => void;
  /** A model of this machine's is gone: drop the local pick if it was that one, and tell the menu. */
  dropLocal: (id: number) => void;
}

function sameMachine(group: PairedGroup | null, model: ModelRef): boolean {
  const machine = group?.machine;
  return (
    machine?.kind === 'paired' &&
    model.machine.kind === 'paired' &&
    machine.fingerprint === model.machine.fingerprint
  );
}

export function useLibrarySelection(group: PairedGroup | null): LibrarySelection {
  const [localPick, setLocalPick] = useState<number | null>(null);
  const [picked, setPicked] = useState<ModelRef | null>(null);
  const farPick = picked !== null && sameMachine(group, picked) ? picked : null;

  // Dropped rather than hidden, so rows coming back do not bring it with them.
  useEffect(() => {
    if (picked !== null && farPick === null) setPicked(null);
  }, [picked, farPick]);

  // The local pick, held here and told to the native menu (a no-op in web mode).
  const holdLocal = useCallback((id: number | null) => {
    setLocalPick(id);
    setSelectedModel(id).catch((err) => {
      appLogger.warn('hook.models', 'Failed to sync model selection with menu', { error: err });
    });
  }, []);

  const pickLocal = useCallback(
    (id: number | null) => {
      setPicked(null);
      holdLocal(id);
    },
    [holdLocal],
  );

  const pickFar = useCallback(
    (model: ModelRef) => {
      setPicked(model);
      holdLocal(null);
    },
    [holdLocal],
  );

  const dropLocal = useCallback(
    (id: number) => {
      if (localPick === id) holdLocal(null);
    },
    [localPick, holdLocal],
  );

  return { localPick, farPick, pickFar, pickLocal, dropLocal };
}
