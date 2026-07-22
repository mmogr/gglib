import { FC, ReactNode } from 'react';
import { cn } from '../../../utils/cn';

interface InfoRowProps {
  label: string;
  children: ReactNode;
  /** Render the value in the monospace face (paths, hashes, filenames). */
  mono?: boolean;
  /** Extra classes for the value cell. */
  className?: string;
  /** Extra classes for the label cell (raw GGUF keys are mono). */
  labelClassName?: string;
}

/**
 * One label/value pair in a metadata grid.
 *
 * Relies on the parent supplying `grid-cols-[minmax(0,9rem)_1fr]`, so every
 * value in a section starts at the same x position. The previous
 * flex/justify-between approach pushed values to the far right edge, which
 * left a wide empty gutter on a panel of any real width.
 */
export const InfoRow: FC<InfoRowProps> = ({ label, children, mono, className, labelClassName }) => (
  <>
    <dt className={cn('text-text-muted text-sm min-w-0', labelClassName)}>{label}</dt>
    <dd
      className={cn(
        'text-text text-sm m-0 min-w-0 break-words',
        mono && 'font-mono text-xs break-all',
        className,
      )}
    >
      {children}
    </dd>
  </>
);
