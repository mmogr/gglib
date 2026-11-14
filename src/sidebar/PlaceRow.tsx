/**
 * One sidebar row for a place: an icon column, the name, the state at the
 * right edge, and any detail on lines under the name.
 *
 * Every row has the same three columns, `[icon] name … word`, so the names
 * share one left edge and the words one right edge, whatever a row says. A
 * state's shape is a badge on the icon's corner, so it sits in the same spot
 * on every row and never pushes a name aside; the word beside it at the
 * right keeps state a shape and a word. A row with no icon (Settings,
 * Diagnostics) keeps the column empty, so its name still lines up.
 *
 * `under` lines sit inside the button and open the place with it; `after`
 * lines sit under the button, for a line holding a control of its own (the
 * endpoint's ⧉), since a button cannot hold another. The fill covers both,
 * a neutral raised fill when the row is the open place, no bar, no colour.
 *
 * @module sidebar/PlaceRow
 */

import type { LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';
import { StatusMark, type MarkKind } from '../components/status/StatusMark';
import { Button } from '../components/ui/Button';
import { Icon } from '../components/ui/Icon';
import { cn } from '../utils/cn';

/** The badge on a row's icon: its shape, the word it stands for, and the endpoint's request blink. */
export interface Badge {
  kind: MarkKind;
  label: string;
  blink?: boolean;
}

interface PlaceRowProps {
  name: string;
  icon?: LucideIcon;
  badge?: Badge | null;
  /** What the row says at its right edge. */
  word?: ReactNode;
  /** Lines under the name that open the place with the row. */
  under?: ReactNode;
  /** Lines under the name that hold a control of their own. */
  after?: ReactNode;
  current: boolean;
  onClick: () => void;
  /** The button's accessible name, when the row says more than its name. */
  label?: string;
  tertiary?: boolean;
}

/** The three columns every row shares: icon, name, word. */
const COLUMNS = 'grid w-full grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-x-sm';

export function PlaceRow({ name, icon, badge = null, word, under, after, current, onClick, label, tertiary = false }: PlaceRowProps) {
  return (
    <div className="px-sm">
      <div
        className={cn(
          'rounded-base bg-[var(--place-bg)] [--place-bg:var(--color-background-secondary)] hover:[--place-bg:var(--color-surface-elevated)]',
          current && '[--place-bg:var(--color-surface-hover)] hover:[--place-bg:var(--color-surface-hover)]',
        )}
      >
        <Button
          variant="ghost"
          fullWidth
          className="h-auto px-sm py-sm font-normal hover:bg-transparent [&>span]:w-full"
          onClick={onClick}
          aria-current={current ? 'page' : undefined}
          aria-label={label}
        >
          <span className={cn(COLUMNS, 'gap-y-[2px] text-left')}>
            <span className="relative flex h-4 w-4 text-text-secondary">
              {icon && <Icon icon={icon} size={16} />}
              {badge && (
                <span className="absolute -right-[3px] -bottom-[3px] flex rounded-full bg-[var(--place-bg)] p-[2px]">
                  <StatusMark kind={badge.kind} label={badge.label} blink={badge.blink} />
                </span>
              )}
            </span>
            <span className={cn('truncate text-sm font-medium', tertiary ? 'text-text-muted' : 'text-text')}>{name}</span>
            <span className="flex items-center gap-xs justify-self-end text-sm text-text-secondary tabular-nums whitespace-nowrap">
              {word}
            </span>
            {under && <span className="col-span-2 col-start-2 flex min-w-0 flex-col text-xs text-text-muted">{under}</span>}
          </span>
        </Button>
        {after && (
          <div className={cn(COLUMNS, 'px-sm pb-sm -mt-xs')}>
            <div className="col-span-2 col-start-2 flex min-w-0 flex-col gap-[2px] text-xs text-text-muted">{after}</div>
          </div>
        )}
      </div>
    </div>
  );
}
