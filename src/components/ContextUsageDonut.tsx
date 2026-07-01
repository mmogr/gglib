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
  className?: string;
}

export const ContextUsageDonut: FC<ContextUsageDonutProps> = ({
  used,
  total,
  size = 96,
  strokeWidth = 10,
  label,
  className,
}) => {
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const fraction = used != null && total ? Math.min(Math.max(used / total, 0), 1) : 0;
  const dashOffset = circumference * (1 - fraction);
  const pct = Math.round(fraction * 100);

  const strokeClass = pct >= 90 ? 'stroke-danger' : pct >= 70 ? 'stroke-warning' : 'stroke-success';

  return (
    <div
      className={cn('relative inline-flex items-center justify-center shrink-0', className)}
      style={{ width: size, height: size }}
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
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
          className={cn('transition-[stroke-dashoffset] duration-500 ease-out', strokeClass)}
        />
      </svg>
      <div className="absolute inset-0 flex flex-col items-center justify-center">
        <span className="text-sm font-semibold text-text">{used != null && total ? `${pct}%` : '—'}</span>
        {label && <span className="text-[10px] text-text-muted">{label}</span>}
      </div>
    </div>
  );
};

export default ContextUsageDonut;
