/**
 * One model, as a library row: its name, one line under it, at most one
 * accessory at the right, and the actions a hover (or focus) reveals.
 *
 * A row is an option of its group's listbox. Picking one changes its fill
 * and nothing else (LB-1): the row's box is the same picked or not, so no
 * text moves. It is a `div`, not a button, because its actions are buttons
 * and a button may not hold another; Enter and Space pick it as a click does.
 * The list is one Tab stop: the picked row's, else the first (`tabStop`),
 * and ↑ ↓ move between rows (`Listbox`).
 *
 * `missingParts` is the accessory for a model that cannot run yet; the
 * triangle is `StatusMark`'s, the words this row's.
 *
 * @module places/library/ModelRow
 */

import type { FC, KeyboardEvent, ReactNode } from 'react';
import { cn } from '../../utils/cn';

interface ModelRowProps {
  name: string;
  /** The facts under the name, joined with " · "; empty ones are dropped. */
  facts: (string | null | undefined | false)[];
  /** At most one: a state, or a line that wants attention. */
  accessory?: ReactNode;
  /** Shown on hover and focus, at the row's right, over the accessory. */
  actions?: ReactNode;
  selected: boolean;
  /** `tabStop`'s: 0 for the list's one Tab stop, -1 for the rest. */
  tabIndex: 0 | -1;
  onSelect: () => void;
}

export const ModelRow: FC<ModelRowProps> = ({ name, facts, accessory, actions, selected, tabIndex, onSelect }) => {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSelect();
    }
  };
  const line = facts.filter(Boolean).join(' · ');
  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={tabIndex}
      onClick={onSelect}
      onKeyDown={onKeyDown}
      className={cn(
        'group relative flex min-h-[52px] cursor-pointer items-center gap-md rounded-base px-md py-sm outline-none',
        'hover:bg-background-hover focus-visible:bg-background-hover focus-within:bg-background-hover',
        selected && 'bg-surface-hover hover:bg-surface-hover focus-within:bg-surface-hover',
      )}
    >
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm text-text">{name}</span>
        {line && <span className="truncate text-xs text-text-secondary tabular-nums">{line}</span>}
      </div>
      {accessory && <span className="shrink-0 group-hover:invisible group-focus-within:invisible">{accessory}</span>}
      {actions && (
        // Over the row's end on the row's own colour, faded in from the left, so a long name ends under it rather than through it.
        <span className="absolute right-sm top-1/2 flex -translate-y-1/2 items-center gap-xs bg-inherit pl-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 before:absolute before:inset-y-0 before:right-full before:w-md before:bg-inherit before:[mask-image:linear-gradient(to_right,transparent,black)]">
          {actions}
        </span>
      )}
    </div>
  );
};
