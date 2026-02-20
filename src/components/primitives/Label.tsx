import React from 'react';
import { cn } from '../../utils/cn';

interface LabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
  children: React.ReactNode;
  size?: 'xs' | 'sm' | 'base';
  muted?: boolean;
}

const sizeClasses = {
  xs: 'text-xs',
  sm: 'text-sm',
  base: '',
} as const;

/**
 * Label - Consistent form label with semantic HTML
 * Default: font-semibold text-text
 * muted: text-text-secondary + uppercase tracking
 */
export const Label: React.FC<LabelProps> = ({
  children,
  className,
  size = 'base',
  muted = false,
  ...props
}) => {
  return (
    <label
      className={cn(
        'font-semibold',
        muted ? 'text-text-secondary uppercase' : 'text-text',
        sizeClasses[size],
        className,
      )}
      {...props}
    >
      {children}
    </label>
  );
};

Label.displayName = 'Label';
