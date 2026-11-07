/**
 * Rows moved through with the arrow keys: one Tab stop for the whole list
 * (a roving tabindex), and ↑ ↓ Home End moving the focus and the pick
 * together, kept inside the list.
 *
 * `listMove` is the one answer to where those keys go, the Load a model…
 * palette's too, whose highlight moves while its field keeps the focus.
 * `Listbox` moves the focus itself: a row is a direct child of the list
 * with `role="option"`, and its `tabIndex` is `tabStop`'s, 0 for the picked
 * row (or the first while none is) and -1 for the rest. A key pressed on
 * something inside a row (its buttons, its ⋯) moves nothing.
 *
 * A key's pick from the picked row replaces it in the history rather than
 * adding one (`replace`), so walking a list with the keys leaves one entry
 * and Back leaves the list; the first pick of a list, from a row not yet
 * picked, adds one as a click does, so Back returns to the place.
 *
 * @module components/ui/Listbox
 */

import type { FC, KeyboardEvent, ReactNode } from 'react';

/**
 * Where ↑ ↓ Home End take a highlight among `count` rows from row `at`,
 * never past the first or the last; `null` for any other key.
 */
export function listMove(key: string, at: number, count: number): number | null {
  let to: number;
  switch (key) {
    case 'ArrowDown':
      to = at + 1;
      break;
    case 'ArrowUp':
      to = at - 1;
      break;
    case 'Home':
      to = 0;
      break;
    case 'End':
      to = count - 1;
      break;
    default:
      return null;
  }
  return Math.max(0, Math.min(count - 1, to));
}

/** Row `index`'s `tabIndex`, `picked` the picked row's index or -1. */
export const tabStop = (index: number, picked: number): 0 | -1 => (index === Math.max(0, picked) ? 0 : -1);

interface ListboxProps {
  /** The list's accessible name. */
  label: string;
  /**
   * Pick row `index`, as a click on it does; `replace` when the row the key
   * was pressed on was the picked one.
   */
  onPick: (index: number, replace: boolean) => void;
  children: ReactNode;
}

export const Listbox: FC<ListboxProps> = ({ label, onPick, children }) => {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const rows = Array.from(event.currentTarget.children).filter(
      (el): el is HTMLElement => el instanceof HTMLElement && el.getAttribute('role') === 'option',
    );
    const at = rows.indexOf(event.target as HTMLElement);
    if (at < 0) return;
    const to = listMove(event.key, at, rows.length);
    if (to === null) return;
    event.preventDefault();
    rows[to].focus();
    if (to !== at) onPick(to, rows[at].getAttribute('aria-selected') === 'true');
  };
  return (
    <div role="listbox" aria-label={label} onKeyDown={onKeyDown}>
      {children}
    </div>
  );
};
