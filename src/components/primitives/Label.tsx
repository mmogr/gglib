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
 * Default: font-medium text-text
 * muted: font-medium text-text-muted (sentence case — the all-caps
 * micro-label treatment is retired; hierarchy comes from size and color)
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
        'font-medium',
        muted ? 'text-text-muted' : 'text-text',
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
