/**
 * ContextUsageDonut.
 *
 * Pure-SVG donut graph showing "tokens in use" vs. a slot's context size.
 * Built with `<circle stroke-dasharray stroke-dashoffset>` and styled with
 * Tailwind — no charting dependency.
 *
 * @module components/ContextUsageDonut
 */

import type { FC } from 'react';
import { cn } from '../utils/cn';
import { usagePercent, usageSeverity, type UsageSeverity } from '../utils/contextUsage';

export interface ContextUsageDonutProps {
  /** Tokens currently in use (numerator). */
  used: number | null;
  /** Total context size (denominator). Renders an empty ring when unset/zero. */
  total: number | null;
  /** Outer diameter in pixels. */
  size?: number;
  strokeWidth?: number;
  /** Small caption rendered under the percentage, e.g. "Slot 0". */
  label?: string;
  /**
   * The ring alone, for a place too small to hold a figure: no percentage,
   * no dash and no caption, and hidden from assistive tech. Whatever it sits
   * in says the reading.
   */
  compact?: boolean;
  className?: string;
}

// Base ring wears the accent: green is reserved for running/online status,
// not for "usage is fine" — only the 70/90 thresholds speak in semantics.
const STROKE: Record<UsageSeverity, string> = {
  normal: 'stroke-primary',
  warning: 'stroke-warning',
  danger: 'stroke-danger',
};

export const ContextUsageDonut: FC<ContextUsageDonutProps> = ({
  used,
  total,
  size = 96,
  strokeWidth = 10,
  label,
  compact = false,
  className,
}) => {
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const fraction = used != null && total ? Math.min(Math.max(used / total, 0), 1) : 0;
  const dashOffset = circumference * (1 - fraction);
  // The figure and the colour both come from the one whole-number percent.
  const pct = used != null && total ? Math.max(0, usagePercent(used, total)) : 0;

  return (
    <span
      className={cn('relative inline-flex items-center justify-center shrink-0', className)}
      style={{ width: size, height: size }}
    >
      <svg
        width={size}
        height={size}
        viewBox={`0 0 ${size} ${size}`}
        className="-rotate-90"
        aria-hidden={compact || undefined}
      >
        <circle cx={size / 2} cy={size / 2} r={radius} fill="none" strokeWidth={strokeWidth} className="stroke-border" />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={strokeWidth}
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={dashOffset}
          className={cn('transition-[stroke-dashoffset] duration-500 ease-out', STROKE[usageSeverity(pct)])}
        />
      </svg>
      {!compact && (
        <span className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="text-sm font-semibold text-text">{used != null && total ? `${pct}%` : '—'}</span>
          {label && <span className="text-2xs text-text-muted">{label}</span>}
        </span>
      )}
    </span>
  );
};

export default ContextUsageDonut;
