/**
 * One row of How it runs (or How it ran): a label in a fixed column, the
 * value with whose choice it was and why on hover, `set by you · Reset`
 * when the person set it, and `Change` on hover or focus, which turns the
 * value into its control in place.
 *
 * @module inspector/local/Row
 */

import type { FC, ReactNode } from 'react';
import { CHOICE_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { cn } from '../../utils/cn';

/** A text button set in a line of text: no height or padding of its own. */
const INLINE = 'align-baseline text-xs font-normal leading-5';

interface RowProps {
  label: string;
  value: ReactNode;
  /** Said on hover: whose choice the value was, and why. */
  why?: string;
  /** The person set this row: `Reset` puts it back. */
  onReset?: () => void;
  /** Offered on hover: turn the value into its control. */
  onChange?: () => void;
  /** The control, while the row is being changed. */
  control?: ReactNode;
}

export const Row: FC<RowProps> = ({ label, value, why, onReset, onChange, control }) => (
  <div className="group relative grid grid-cols-[6.5rem_minmax(0,1fr)] items-baseline gap-x-md py-xs" aria-label={label} role="group">
    <dt className="text-xs leading-5 text-text-secondary">{label}</dt>
    <dd className="m-0 flex min-w-0 flex-col gap-xs text-xs leading-5 text-text">
      {control ?? (
        <span className={cn('min-w-0', onChange && 'pr-14')}>
          <span className="break-words tabular-nums" title={why}>
            {value}
          </span>
          {onReset && (
            <span className="ml-sm text-text-secondary">
              {`set by ${CHOICE_WORDS.you} · `}
              <Button variant="link" size="sm" className={INLINE} onClick={onReset}>
                Reset
              </Button>
            </span>
          )}
        </span>
      )}
    </dd>
    {onChange && !control && (
      // Out of the flow, so a hidden Change takes no line and no height.
      <Button
        variant="link"
        size="sm"
        className={cn(INLINE, 'absolute right-0 top-xs opacity-0 group-hover:opacity-100 focus-visible:opacity-100')}
        onClick={onChange}
        aria-label={`Change ${label}`}
      >
        Change
      </Button>
    )}
  </div>
);
