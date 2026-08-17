/**
 * Re-derive a model's capability tags from its GGUF.
 *
 * Lifted out of `ModelInspectorPanel` unchanged: it is a self-contained
 * confirm → call → toast → reload sequence with its own `retagging` flag, and
 * the panel is a composition root that was carrying forty lines of it.
 */

import { useCallback, useState } from 'react';
import { retagModel } from '../../../services/transport/api/models/local';
import type { ToastType } from '../../Toast/Toast';
import type { ConfirmOptions } from '../../ui/ConfirmDialog';

interface UseRetagModelConfig {
  modelId: number | null | undefined;
  /** Re-read the model detail once the tags have changed. */
  reload: () => Promise<void>;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
}

export interface RetagModelState {
  retagging: boolean;
  /** `full` rebuilds every detected tag; otherwise only missing ones are added. */
  handleRetag: (full: boolean) => Promise<void>;
}

export function useRetagModel({
  modelId,
  reload,
  showToast,
  confirm,
}: UseRetagModelConfig): RetagModelState {
  const [retagging, setRetagging] = useState(false);

  const handleRetag = useCallback(
    async (full: boolean) => {
      if (modelId == null) return;

      // Rebuild drops detected tags the GGUF no longer yields, hand-added ones
      // included — and those tags decide launch flags and response parsing.
      if (full) {
        const confirmed = await confirm({
          title: 'Rebuild detected tags?',
          description:
            'Re-derives the capability tags and dialect spec from the GGUF. Detected tags it no ' +
            'longer finds are dropped, including any you added by hand — a manual "reasoning" tag ' +
            'forcing a reasoning format would be lost. Tags outside that set are untouched.',
          confirmLabel: 'Rebuild',
          variant: 'danger',
        });
        if (!confirmed) return;
      }

      setRetagging(true);
      try {
        const diff = await retagModel(modelId, full);
        if (!diff.changed) {
          showToast('Tags already up to date', 'success');
        } else {
          const parts = [
            diff.added.length > 0 ? `added ${diff.added.length}` : null,
            diff.removed.length > 0 ? `removed ${diff.removed.length}` : null,
            diff.specChanged ? 'dialect spec re-derived' : null,
          ].filter(Boolean);
          // Counts, not full lists: the chips reload right below, and a
          // removal is the half worth pausing on, so it gets longer on screen.
          showToast(
            `Retagged: ${parts.join(', ')}`,
            'success',
            diff.removed.length > 0 ? 8000 : undefined,
          );
        }
        await reload();
      } catch (err) {
        showToast(err instanceof Error ? err.message : String(err), 'error');
      } finally {
        setRetagging(false);
      }
    },
    [modelId, reload, showToast, confirm],
  );

  return { retagging, handleRetag };
}
