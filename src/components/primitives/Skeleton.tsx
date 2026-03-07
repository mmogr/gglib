import React from 'react';
import { cn } from '../../utils/cn';
import { Stack } from './Stack';

export interface SkeletonProps {
  /** Visual shape of the skeleton block. Defaults to 'rect'. */
  variant?: 'text' | 'rect' | 'circle';
  /** CSS width value, e.g. '60%' or '120px'. Defaults vary by variant. */
  width?: string;
  /** CSS height value, e.g. '1em' or '32px'. Defaults vary by variant. */
  height?: string;
  className?: string;
  /** Render N stacked skeleton items with gap-sm between them. */
  count?: number;
}

const SHIMMER_BG = {
  background:
    'linear-gradient(90deg, var(--color-surface-elevated) 25%, var(--color-surface-hover) 50%, var(--color-surface-elevated) 75%)',
  backgroundSize: '200% 100%',
} as const;

const variantClasses: Record<NonNullable<SkeletonProps['variant']>, string> = {
  text: 'rounded-sm',
  rect: 'rounded-base',
  circle: 'rounded-full shrink-0',
};

const variantDefaults: Record<
  NonNullable<SkeletonProps['variant']>,
  { width: string; height: string }
> = {
  text: { width: '100%', height: '1em' },
  rect: { width: '100%', height: '1rem' },
  circle: { width: '2rem', height: '2rem' },
};

export const Skeleton: React.FC<SkeletonProps> = ({
  variant = 'rect',
  width,
  height,
  className,
  count = 1,
}) => {
  const defaults = variantDefaults[variant];
  const w = width ?? defaults.width;
  const h = height ?? defaults.height;
  const itemClass = cn('animate-shimmer', variantClasses[variant], className);
  const itemStyle = { ...SHIMMER_BG, width: w, height: h };

  if (count > 1) {
    return (
      <div aria-hidden="true">
        <Stack gap="sm">
          {Array.from({ length: count }, (_, i) => (
            <div key={i} className={itemClass} style={itemStyle} />
          ))}
        </Stack>
      </div>
    );
  }

  return <div aria-hidden="true" className={itemClass} style={itemStyle} />;
};

Skeleton.displayName = 'Skeleton';
