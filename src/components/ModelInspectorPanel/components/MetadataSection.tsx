import { FC, ReactNode } from 'react';
import { cn } from '../../../utils/cn';

interface MetadataSectionProps {
  title: string;
  children: ReactNode;
  className?: string;
}

/**
 * A titled definition grid.
 *
 * Owns the vertical rhythm between sections so headings can't end up flush
 * against the last row of the section above them, and fixes the label column
 * width so values align down the panel.
 */
export const MetadataSection: FC<MetadataSectionProps> = ({ title, children, className }) => (
  <div className={cn('mt-xl first:mt-0', className)}>
    <h3 className="m-0 mb-md text-xs font-semibold text-text-secondary uppercase tracking-[0.05em]">
      {title}
    </h3>
    <dl className="grid grid-cols-[minmax(0,9rem)_1fr] gap-x-base gap-y-md m-0">
      {children}
    </dl>
  </div>
);
