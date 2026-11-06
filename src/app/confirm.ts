/**
 * The confirmation dialog's question, asked from anywhere.
 *
 * `confirm(opts)` resolves to the person's answer. It is a store, not only a
 * context, because the native menu asks it from outside React (Remove this
 * model?), and because a test can answer it without a dialog. `ConfirmProvider`
 * (`contexts/ConfirmContext`) draws the question in the store and writes the
 * answer back; `useConfirmContext()` still hands components `confirm`.
 *
 * One question at a time: a second asked while one is open is answered
 * `false` at once rather than orphaning the first.
 *
 * @module app/confirm
 */

import type { ConfirmOptions } from '../components/ui/ConfirmDialog';
import { createEventStore } from '../services/createEventStore';

export interface PendingConfirm {
  opts: ConfirmOptions;
  /** Answer it; the dialog closes. A question already answered takes no second answer. */
  answer: (value: boolean) => void;
}

const store = createEventStore<PendingConfirm | null>(null);

/** Ask, and get the answer. */
export function confirm(opts: ConfirmOptions): Promise<boolean> {
  if (store.getState() !== null) return Promise.resolve(false);
  return new Promise<boolean>((resolve) => {
    const pending: PendingConfirm = {
      opts,
      answer: (value) => {
        if (store.getState() !== pending) return;
        store.setState(null);
        resolve(value);
      },
    };
    store.setState(pending);
  });
}

/** Answer the open question `false`, if one is open: the asker is going away. */
export function dismissConfirm(): void {
  store.getState()?.answer(false);
}

/** React hook: the question open now, or `null`. */
export function usePendingConfirm(): PendingConfirm | null {
  return store.useStore();
}
