import React from 'react';
import { cn } from '../../utils/cn';

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
  const baseClasses = 'rounded-md';
  
  const variantClasses = {
    default: 'bg-surface border border-border',
    elevated: 'bg-surface border border-border shadow-md',
    outlined: 'border-2 border-border',
  };

  const paddingClasses = {
    none: '',
    sm: 'p-sm',
    base: 'p-base',
    lg: 'p-lg',
  };

  return (
    <div
      className={cn(baseClasses, variantClasses[variant], paddingClasses[padding], className)}
    >
      {children}
    </div>
  );
};
