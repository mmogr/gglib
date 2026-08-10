import React from 'react';
import { cn } from '../../utils/cn';

export type ReadoutIntent = 'neutral' | 'accent' | 'warning' | 'danger';
export type ReadoutSize = 'sm' | 'md' | 'lg';

interface ReadoutProps {
  label: string;
  value: string | number;
  /** Quiet suffix rendered beside the value (e.g. "tok/s", "%", "MiB"). */
  unit?: string;
  /** Colors the value only — labels and units always stay quiet. */
  intent?: ReadoutIntent;
  size?: ReadoutSize;
  /** Slot for a Sparkline (or other trend mark) rendered under the value. */
  trend?: React.ReactNode;
  align?: 'start' | 'end';
  className?: string;
}

const intentClasses: Record<ReadoutIntent, string> = {
  neutral: 'text-text',
  accent: 'text-primary',
  warning: 'text-warning',
  danger: 'text-danger',
};

const sizeClasses: Record<ReadoutSize, string> = {
  sm: 'text-sm',
  md: 'text-lg',
  lg: 'text-2xl',
};

/**
 * Readout - the single treatment for a live metric.
 *
 * Values wear mono + tabular figures so columns of telemetry align. Per the
 * design contracts every live metric renders through a Readout, and a
 * Sparkline never appears without one — pass it through `trend`.
 */
export const Readout: React.FC<ReadoutProps> = ({
  label,
  value,
  unit,
  intent = 'neutral',
  size = 'md',
  trend,
  align = 'start',
  className,
}) => (
  <div className={cn('flex min-w-0 flex-col', align === 'end' && 'items-end text-right', className)}>
    <span className="text-xs text-text-muted">{label}</span>
    <span
      className={cn(
        'font-mono font-medium tabular-nums leading-tight',
        sizeClasses[size],
        intentClasses[intent],
      )}
    >
      {value}
      {unit ? <span className="text-2xs font-normal text-text-muted"> {unit}</span> : null}
    </span>
    {trend ? <span className="mt-xs">{trend}</span> : null}
  </div>
);
