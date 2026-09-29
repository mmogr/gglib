import { FC, ReactNode } from 'react';
import { cn } from '../../../utils/cn';

interface TurnRowProps {
  /** Who and when: the margin's head. */
  who: ReactNode;
  body: ReactNode;
  /** How it was made: the margin's foot, read after the body. */
  made?: ReactNode;
  className?: string;
}

/**
 * One row of the notebook: a right-aligned margin, then the body.
 *
 * The reading order is who, body, how it was made; the margin is one column
 * only visually. Where the notebook is too narrow for both columns (a
 * container query, so an open conversation list counts), the margin's
 * content moves above the body instead of being cut off.
 */
export const TurnRow: FC<TurnRowProps> = ({ who, body, made, className }) => (
  <div
    className={cn(
      'grid grid-cols-1 gap-y-sm py-lg border-t border-border-light',
      '@min-[40rem]:grid-cols-[220px_minmax(0,1fr)] @min-[40rem]:grid-rows-[auto_1fr] @min-[40rem]:gap-x-xl',
      className,
    )}
  >
    <div className="row-start-1 @min-[40rem]:col-start-1 flex flex-row flex-wrap items-baseline gap-x-md gap-y-xs text-xs text-text-muted @min-[40rem]:flex-col @min-[40rem]:items-end @min-[40rem]:text-right">
      {who}
    </div>
    <div className="row-start-3 min-w-0 max-w-[680px] @min-[40rem]:col-start-2 @min-[40rem]:row-span-2 @min-[40rem]:row-start-1">
      {body}
    </div>
    {made && (
      <div className="row-start-2 flex flex-row flex-wrap items-center gap-x-md gap-y-sm text-xs text-text-muted @min-[40rem]:col-start-1 @min-[40rem]:flex-col @min-[40rem]:items-end @min-[40rem]:pt-sm @min-[40rem]:text-right">
        {made}
      </div>
    )}
  </div>
);
