import React from 'react';
import { cn } from '../../utils/cn';

interface RowProps {
  children: React.ReactNode;
  className?: string;
  gap?: 'none' | 'xs' | 'sm' | 'base' | 'lg' | 'xl';
  align?: 'start' | 'center' | 'end' | 'stretch';
  justify?: 'start' | 'center' | 'end' | 'between' | 'around';
  wrap?: boolean;
}

const gapClasses = {
  none: 'gap-0',
  xs: 'gap-1',
  sm: 'gap-2',
  base: 'gap-4',
  lg: 'gap-6',
  xl: 'gap-8',
} as const;

const alignClasses = {
  start: 'items-start',
  center: 'items-center',
  end: 'items-end',
  stretch: 'items-stretch',
} as const;

const justifyClasses = {
  start: 'justify-start',
  center: 'justify-center',
  end: 'justify-end',
  between: 'justify-between',
  around: 'justify-around',
} as const;

/**
 * Row - Horizontal layout container with consistent spacing
 * Uses Tailwind flex utilities
 */
export const Row: React.FC<RowProps> = ({
  children,
  className,
  gap = 'base',
  align = 'center',
  justify = 'start',
  wrap = false,
}) => {
  return (
    <div
      className={cn(
        'flex flex-row',
        gapClasses[gap],
        alignClasses[align],
        justifyClasses[justify],
        wrap && 'flex-wrap',
        className,
      )}
    >
      {children}
    </div>
  );
};

Row.displayName = 'Row';
