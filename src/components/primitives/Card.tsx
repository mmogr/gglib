import React from 'react';

interface CardProps {
  children: React.ReactNode;
  className?: string;
  variant?: 'default' | 'elevated' | 'outlined';
  padding?: 'none' | 'sm' | 'base' | 'lg';
}

/**
 * Card - Container component for grouping related content
 * Uses Tailwind utilities with token-backed colors
 */
export const Card: React.FC<CardProps> = ({
  children,
  className = '',
  variant = 'default',
  padding = 'base',
}) => {
  const baseClasses = 'rounded-lg';
  
  const variantClasses = {
    default: 'bg-[var(--color-surface)] border border-[var(--color-border)]',
    elevated: 'bg-[var(--color-surface)] shadow-md',
    outlined: 'border-2 border-[var(--color-border)]',
  };

  const paddingClasses = {
    none: '',
    sm: 'p-3',
    base: 'p-4',
    lg: 'p-6',
  };

  return (
    <div
      className={`${baseClasses} ${variantClasses[variant]} ${paddingClasses[padding]} ${className}`}
    >
      {children}
    </div>
  );
};
