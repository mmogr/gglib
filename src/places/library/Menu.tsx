/**
 * A button that opens a short menu under it: the library header's `Sort ▾`
 * and `+ ▾`. An item is an action, or a choice that is ticked when it is
 * the one in force. The menu closes on a pick, a click outside, or Escape.
 *
 * @module places/library/Menu
 */

import { useRef, useState, type FC, type ReactNode } from 'react';
import { Check, ChevronDown } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useClickOutside } from '../../hooks/useClickOutside';
import { cn } from '../../utils/cn';

export interface MenuItem {
  label: string;
  onPick: () => void;
  /** A choice: whether it is the one in force. Absent for an action. */
  checked?: boolean;
  /** Draw a gap before this item, between two sets of choices. */
  gapBefore?: boolean;
}

interface MenuProps {
  /** What the button says (its name is this, plus `name` when given). */
  label: ReactNode;
  /** The button's accessible name, when its label is an icon. */
  name?: string;
  items: MenuItem[];
}

export const Menu: FC<MenuProps> = ({ label, name, items }) => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useClickOutside(ref, () => setOpen(false), open);

  return (
    <div className="relative" ref={ref}>
      <Button
        variant="ghost"
        size="sm"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={name}
        rightIcon={<Icon icon={ChevronDown} size={12} />}
        onClick={() => setOpen((o) => !o)}
      >
        {label}
      </Button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full z-popover mt-xs flex min-w-[200px] flex-col rounded-base bg-surface-elevated p-xs shadow-lg"
          onKeyDown={(event) => {
            if (event.key !== 'Escape') return;
            event.preventDefault();
            setOpen(false);
          }}
        >
          {items.map((item) => (
            <Button
              key={item.label}
              variant="ghost"
              size="sm"
              role={item.checked === undefined ? 'menuitem' : 'menuitemradio'}
              aria-checked={item.checked}
              className={cn('justify-start text-sm font-normal text-text', item.gapBefore && 'mt-sm')}
              leftIcon={
                item.checked === undefined ? undefined : (
                  <Icon icon={Check} size={12} className={item.checked ? undefined : 'invisible'} />
                )
              }
              onClick={() => {
                setOpen(false);
                item.onPick();
              }}
            >
              {item.label}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
};
