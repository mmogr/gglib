import React from 'react';
import { cn } from '../../utils/cn';

interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

/**
 * EmptyState - Consistent empty state pattern for lists and collections
 * Uses Tailwind utilities with token-backed colors
 */
export const EmptyState: React.FC<EmptyStateProps> = ({
  icon,
  title,
  description,
  action,
  className = '',
}) => {
  return (
    <div
      className={cn('flex flex-col items-center justify-center gap-base p-2xl text-center', className)}
    >
      {icon && (
        <div className="flex h-12 w-12 items-center justify-center rounded-full bg-surface-elevated border border-border-light text-text-muted">
          {icon}
        </div>
      )}
      <div className="flex flex-col gap-sm">
        <h3 className="text-lg font-semibold text-text">
          {title}
        </h3>
        {description && (
          <p className="text-sm text-text-muted max-w-[28rem]">
            {description}
          </p>
        )}
      </div>
      {action && <div className="mt-sm">{action}</div>}
    </div>
  );
};
