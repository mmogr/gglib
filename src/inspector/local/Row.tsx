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
  <div className="group grid grid-cols-[6.5rem_minmax(0,1fr)] gap-x-md py-xs" aria-label={label} role="group">
    <dt className="text-xs text-text-secondary">{label}</dt>
    <dd className="m-0 flex min-w-0 flex-col gap-xs text-xs text-text">
      {control ?? (
        <span className="flex min-w-0 flex-wrap items-baseline gap-x-sm">
          <span className="min-w-0 break-words tabular-nums" title={why}>
            {value}
          </span>
          {onReset && (
            <span className="text-text-secondary">
              {`set by ${CHOICE_WORDS.you} · `}
              <Button variant="link" size="sm" className="text-xs font-normal" onClick={onReset}>
                Reset
              </Button>
            </span>
          )}
          {onChange && (
            <Button
              variant="link"
              size="sm"
              className="text-xs font-normal opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              onClick={onChange}
              aria-label={`Change ${label}`}
            >
              Change
            </Button>
          )}
        </span>
      )}
    </dd>
  </div>
);
