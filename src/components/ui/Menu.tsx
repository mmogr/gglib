/**
 * A button that opens a short menu beside it: the library header's `Sort ▾`
 * and `+ ▾`, the inspector's ⋯, a chat's title ▾, a sidebar chat's ⋯, and
 * the composer's `[+]` and model picker. An item is an action, a choice
 * ticked when it is the one in force, a box ticked on and off that leaves
 * the menu open, or a submenu (`▸`) that opens beside it; a danger item is
 * drawn as one. A heading names the items after it, as a group, and a note
 * is a line of text that does nothing. A disabled item with a reason says it
 * under itself, in a line it is described by, not only on hover. A menu is
 * as wide as its widest row, between 200px and 24rem (a submenu 20rem, and
 * either 90% of a narrow window): a row's label is one line where it fits and wraps under itself,
 * the row growing with it, where it does not; a note or a reason wraps at
 * the width the rows set. The menu closes on a pick, a click outside, or
 * Escape, and a pick or Escape gives the focus back to its button; Escape in
 * an open submenu closes that submenu alone, the focus back on its item.
 *
 * @module components/ui/Menu
 */

import { Fragment, useId, useRef, useState, type FC, type ReactNode } from 'react';
import { Check, ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from './Button';
import { Icon } from './Icon';
import { useClickOutside } from '../../hooks/useClickOutside';
import { cn } from '../../utils/cn';

export interface MenuItem {
  label: string;
  /** What a pick does; absent for a heading, a note or a submenu. */
  onPick?: () => void;
  /** A choice: whether it is the one in force. Absent for an action. */
  checked?: boolean;
  /** Leaves the menu open: with `checked`, a box ticked on and off, not one choice of several. */
  toggles?: boolean;
  /** Draw a gap before this item, between two sets of choices. */
  gapBefore?: boolean;
  /** An action that removes something: drawn in the danger colour. */
  danger?: boolean;
  /** Said on hover: what the action does, where its label cannot. */
  title?: string;
  /** Offered but not now: drawn greyed, with why in `title` when given. */
  disabled?: boolean;
  /** The items a pick opens beside this one. */
  submenu?: MenuItem[];
  /** The name of the group of items after it, up to the next heading. */
  heading?: boolean;
  /** A line of text, not an item. */
  note?: boolean;
}

interface MenuProps {
  /** What the button says (its name is this, plus `name` when given). */
  label: ReactNode;
  /** The button's accessible name, when its label is an icon. */
  name?: string;
  items: MenuItem[];
  /** The button is its icon alone, with no ▾ (the inspector's ⋯). */
  iconOnly?: boolean;
  /** Which edge of the button the menu hangs from: its right (the default) or its left. */
  align?: 'start' | 'end';
  /** Whether the menu opens under the button (the default) or over it, for a button at the foot of the window. */
  side?: 'below' | 'above';
  /** The button's own look, where it is more than a control (a chat's title). */
  className?: string;
  /** Said on the button's hover. */
  title?: string;
  /** The button is greyed and opens nothing. */
  disabled?: boolean;
  /** Called as the menu opens, for items read fresh each time. */
  onOpen?: () => void;
}

/**
 * A line of text in a menu, a note or a disabled item's reason: it wraps at
 * the width the rows give the menu and adds nothing to it, so a sentence
 * does not make a menu of short rows a wide one.
 */
const PROSE = 'm-0 w-0 min-w-full px-2.5 text-xs text-text-secondary';

interface Group {
  heading?: string;
  items: MenuItem[];
}

/** The items as groups: those before the first heading, then each heading with the items after it. */
function groupsOf(items: MenuItem[]): Group[] {
  const groups: Group[] = [{ items: [] }];
  for (const item of items) {
    if (item.heading) groups.push({ heading: item.label, items: [] });
    else groups[groups.length - 1].items.push(item);
  }
  return groups.filter((group) => group.heading !== undefined || group.items.length > 0);
}

/** The role an item is read as. */
function roleOf(item: MenuItem): string {
  if (item.submenu || item.checked === undefined) return 'menuitem';
  return item.toggles ? 'menuitemcheckbox' : 'menuitemradio';
}

interface ListProps {
  label?: string;
  items: MenuItem[];
  className: string;
  /** Where a submenu opens: to the right of its item, or to its left. */
  toward: 'right' | 'left';
  side: 'below' | 'above';
  close: () => void;
}

const MenuList: FC<ListProps> = ({ label, items, className, toward, side, close }) => {
  const [openSub, setOpenSub] = useState<string | null>(null);
  const baseId = useId();
  /** The items that open a submenu, by label: Escape there gives each its focus back. */
  const subItems = useRef(new Map<string, HTMLButtonElement>());

  const entry = (item: MenuItem, index: number) => {
    const key = `${index}:${item.label}`;
    if (item.note) {
      return (
        <p key={key} className={cn(PROSE, 'py-xs', item.gapBefore && 'mt-sm')}>
          {item.label}
        </p>
      );
    }
    const sub = item.submenu;
    const reasonId = item.disabled && item.title ? `${baseId}-${index}` : undefined;
    const button = (
      <Button
        ref={
          sub
            ? (element: HTMLButtonElement | null) => {
                if (element) subItems.current.set(item.label, element);
                else subItems.current.delete(item.label);
              }
            : undefined
        }
        aria-describedby={reasonId}
        type="button"
        variant="ghost"
        size="sm"
        role={roleOf(item)}
        aria-checked={sub ? undefined : item.checked}
        aria-haspopup={sub ? 'menu' : undefined}
        aria-expanded={sub ? openSub === item.label : undefined}
        title={item.title}
        disabled={item.disabled}
        className={cn(
          // As tall as its label: one line is the button's own 28px (a 20px
          // line, 3px above and below it, and the 1px border), and a label too
          // long for the menu wraps under itself, not over the next row.
          'h-auto min-h-7 w-full justify-start py-[3px] text-left text-sm font-normal text-text',
          item.danger && 'text-danger hover:text-danger',
          item.gapBefore && 'mt-sm',
        )}
        leftIcon={
          item.checked === undefined ? undefined : (
            <Icon icon={Check} size={12} className={item.checked ? undefined : 'invisible'} />
          )
        }
        rightIcon={sub ? <Icon icon={ChevronRight} size={12} className="ml-auto" /> : undefined}
        onClick={() => {
          if (sub) {
            setOpenSub((open) => (open === item.label ? null : item.label));
            return;
          }
          if (!item.toggles) close();
          item.onPick?.();
        }}
      >
        {item.label}
      </Button>
    );
    const reason = reasonId && (
      <p id={reasonId} className={cn(PROSE, 'pb-xs')}>
        {item.title}
      </p>
    );
    if (!sub) return <Fragment key={key}>{button}{reason}</Fragment>;
    return (
      <div key={key} className="relative">
        {button}
        {reason}
        {openSub === item.label && (
          <MenuList
            label={item.label}
            items={sub}
            toward={toward}
            side={side}
            close={close}
            className={cn(
              toward === 'right' ? 'left-full ml-xs' : 'right-full mr-xs',
              side === 'above' ? 'bottom-0' : 'top-0',
              // Narrower than a menu, since it starts a menu's width in: beside
              // the composer's `[+]` menu, 20rem ends inside the chat at the
              // app's smallest window.
              'max-w-[min(20rem,90vw)]',
            )}
          />
        )}
      </div>
    );
  };

  return (
    <div
      role="menu"
      aria-label={label}
      onKeyDown={(event) => {
        // A submenu open here closes first, and its item takes the focus.
        if (event.key !== 'Escape' || openSub === null) return;
        event.preventDefault();
        event.stopPropagation();
        subItems.current.get(openSub)?.focus();
        setOpenSub(null);
      }}
      className={cn(
        // As wide as its widest row, up to a width a row's label wraps at:
        // left to itself, a box hung beside its button is as narrow as its
        // words allow, and a label wider than the 200px floor wraps.
        'absolute z-popover flex w-max min-w-[200px] max-w-[min(24rem,90vw)] flex-col rounded-base bg-surface-elevated p-xs wrap-anywhere shadow-lg',
        // A long list scrolls; one with a submenu cannot, since a scrolling
        // box would clip the submenu that opens beside it.
        !items.some((item) => item.submenu) && 'max-h-[70vh] overflow-y-auto',
        className,
      )}
    >
      {groupsOf(items).map((group, g) =>
        group.heading === undefined ? (
          <Fragment key={`group:${g}`}>{group.items.map(entry)}</Fragment>
        ) : (
          <div key={`group:${g}`} role="group" aria-label={group.heading} className={cn('flex flex-col', g > 0 && 'mt-sm')}>
            <span aria-hidden className="px-2.5 py-xs text-xs font-semibold text-text-muted">{group.heading}</span>
            {group.items.map(entry)}
          </div>
        ),
      )}
    </div>
  );
};

export const Menu: FC<MenuProps> = ({
  label,
  name,
  items,
  iconOnly = false,
  align = 'end',
  side = 'below',
  className,
  title,
  disabled,
  onOpen,
}) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  useClickOutside(ref, () => setOpen(false), open);
  /** Close on a pick or Escape: the focus goes back to the button. */
  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  return (
    <div
      className="relative"
      ref={ref}
      onKeyDown={(event) => {
        if (!open || event.key !== 'Escape') return;
        event.preventDefault();
        close();
      }}
    >
      <Button
        ref={trigger}
        type="button"
        variant="ghost"
        size="sm"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={name}
        iconOnly={iconOnly}
        className={className}
        title={title}
        disabled={disabled}
        rightIcon={iconOnly ? undefined : <Icon icon={ChevronDown} size={12} />}
        onClick={() => {
          if (!open) onOpen?.();
          setOpen(!open);
        }}
      >
        {label}
      </Button>
      {open && (
        <MenuList
          items={items}
          toward={align === 'end' ? 'left' : 'right'}
          side={side}
          close={close}
          className={cn(align === 'end' ? 'right-0' : 'left-0', side === 'above' ? 'bottom-full mb-xs' : 'top-full mt-xs')}
        />
      )}
    </div>
  );
};
