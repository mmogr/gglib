import React from 'react';

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
      className={`flex flex-col items-center justify-center gap-4 p-12 text-center ${className}`}
    >
      {icon && (
        <div className="text-[var(--color-text-muted)] text-5xl opacity-50">
          {icon}
        </div>
      )}
      <div className="flex flex-col gap-2">
        <h3 className="text-lg font-semibold text-[var(--color-text)]">
          {title}
        </h3>
        {description && (
          <p className="text-sm text-[var(--color-text-muted)] max-w-md">
            {description}
          </p>
        )}
      </div>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
};
