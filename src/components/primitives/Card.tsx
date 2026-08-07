import React from 'react';
import { cn } from '../../utils/cn';

interface CardProps {
  children: React.ReactNode;
  className?: string;
  variant?: 'surface' | 'elevated';
  padding?: 'none' | 'sm' | 'base' | 'lg';
  /** Selection is expressed as tint + ring — never a layout-shifting border. */
  selected?: boolean;
  /** Adds hover/focus affordances for clickable cards. */
  interactive?: boolean;
}

/**
 * Card - Container component for grouping related content
 *
 * Separates from its background by surface step, not by stroke: in-flow
 * cards are borderless per the design contracts; `elevated` adds shadow
 * (the shadow tokens carry their own hairline ring).
 */
export const Card: React.FC<CardProps> = ({
  children,
  className = '',
  variant = 'surface',
  padding = 'base',
  selected = false,
  interactive = false,
}) => {
  const baseClasses = 'rounded-md';

  const variantClasses = {
    surface: 'bg-surface',
    elevated: 'bg-surface-elevated shadow-md',
  };

  const paddingClasses = {
    none: '',
    sm: 'p-sm',
    base: 'p-base',
    lg: 'p-lg',
  };

  return (
    <div
      className={cn(
        baseClasses,
        variantClasses[variant],
        paddingClasses[padding],
        selected && 'bg-primary-subtle ring-1 ring-primary-border',
        interactive &&
          'cursor-pointer transition-colors hover:bg-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary',
        className,
      )}
    >
      {children}
    </div>
  );
};
