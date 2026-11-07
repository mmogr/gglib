/**
 * A button that opens a short menu beside it: the library header's `Sort ▾`
 * and `+ ▾`, the inspector's ⋯, a chat's title ▾, a sidebar chat's ⋯, and
 * the composer's `[+]` and model picker. An item is an action, a choice
 * ticked when it is the one in force, a box ticked on and off that leaves
 * the menu open, or a submenu (`▸`) that opens beside it; a danger item is
 * drawn as one. A heading names the items after it, as a group, and a note
 * is a line of text that does nothing. A disabled item with a reason says it
 * under itself, in a line it is described by, not only on hover. The menu
 * closes on a pick, a click outside, or Escape, and a pick or Escape gives
 * the focus back to its button; Escape in an open submenu closes that
 * submenu alone, the focus back on its item.
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
        <p key={key} className={cn('m-0 px-2.5 py-xs text-xs text-text-secondary', item.gapBefore && 'mt-sm')}>
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
          'w-full justify-start text-sm font-normal text-text',
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
      <p id={reasonId} className="m-0 px-2.5 pb-xs text-xs text-text-secondary">
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
            className={cn(toward === 'right' ? 'left-full ml-xs' : 'right-full mr-xs', side === 'above' ? 'bottom-0' : 'top-0')}
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
        'absolute z-popover flex min-w-[200px] flex-col rounded-base bg-surface-elevated p-xs shadow-lg',
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
