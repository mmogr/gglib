import { useCallback, useEffect, useState } from 'react';

/**
 * How the conversation list sits beside the notebook.
 *
 * - `open`: visible until folded to the rail.
 * - `folded`: the rail only until unfolded.
 * - `always`: always visible, with no control to fold it.
 *
 * In the first two the choice is remembered, and the list folds by itself
 * while the window is too narrow for it and the notebook.
 */
export type ListPolicy = 'open' | 'folded' | 'always';

/** The chat page's policy. Changing the default is this one line. */
export const LIST_POLICY: ListPolicy = 'open';

/** Where the reader's choice is remembered: `open` or `folded`. */
export const LIST_STORAGE_KEY = 'gglib.chat.list';

/**
 * A window wide enough for rail (72) + list (280) + the notebook's margin
 * (220), gap (32), a body worth reading (480) and its padding (48).
 */
export const LIST_FITS_QUERY = '(min-width: 1132px)';

function readChoice(policy: ListPolicy): boolean {
  try {
    const stored = window.localStorage.getItem(LIST_STORAGE_KEY);
    if (stored === 'open' || stored === 'folded') return stored === 'open';
  } catch {
    // Storage unavailable: fall back to the policy.
  }
  return policy !== 'folded';
}

function writeChoice(open: boolean): void {
  try {
    window.localStorage.setItem(LIST_STORAGE_KEY, open ? 'open' : 'folded');
  } catch {
    // Storage unavailable: the choice lasts this visit only.
  }
}

function mediaFits(): boolean {
  return typeof window.matchMedia === 'function' ? window.matchMedia(LIST_FITS_QUERY).matches : true;
}

/** Whether the window fits the list beside the notebook; true where unknown. */
function useFits(): boolean {
  const [fits, setFits] = useState(mediaFits);
  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return undefined;
    const query = window.matchMedia(LIST_FITS_QUERY);
    const update = () => setFits(query.matches);
    update();
    query.addEventListener('change', update);
    return () => query.removeEventListener('change', update);
  }, []);
  return fits;
}

export interface ListFold {
  /** Whether the list is showing. */
  open: boolean;
  /** Whether there is a control to fold and unfold it. */
  canFold: boolean;
  toggle: () => void;
  /** Show the list, e.g. to search it. */
  unfold: () => void;
}

/**
 * The conversation list's fold. Where the window fits the list, the
 * reader's choice holds and is remembered; where it does not, the list is
 * folded until unfolded, and that is not remembered.
 */
export function useListFold(policy: ListPolicy = LIST_POLICY): ListFold {
  const fits = useFits();
  const [chosen, setChosen] = useState(() => readChoice(policy));
  const [narrowOpen, setNarrowOpen] = useState(false);
  useEffect(() => {
    if (fits) setNarrowOpen(false);
  }, [fits]);

  const always = policy === 'always';
  const open = always || (fits ? chosen : narrowOpen);

  const set = useCallback(
    (next: boolean) => {
      if (always) return;
      if (fits) {
        setChosen(next);
        writeChoice(next);
      } else {
        setNarrowOpen(next);
      }
    },
    [always, fits],
  );

  return {
    open,
    canFold: !always,
    toggle: () => set(!open),
    unfold: () => set(true),
  };
}
