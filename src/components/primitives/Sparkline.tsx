import React from 'react';
import { cn } from '../../utils/cn';

interface SparklineProps {
  values: number[];
  width?: number;
  height?: number;
  /** Fixed domain (e.g. 0–100 for percentages) so the line doesn't autoscale noise. */
  min?: number;
  max?: number;
  strokeWidth?: number;
  ariaLabel: string;
  className?: string;
}

/**
 * Sparkline - the app's single micro-chart style for live telemetry.
 *
 * Pure SVG and non-interactive: the adjacent Readout carries the current
 * value, so the mark needs no tooltip layer. Stroke follows currentColor —
 * set intent with a text color utility at the call site (e.g. text-primary).
 * Renders only the faint baseline until two samples exist.
 */
export const Sparkline: React.FC<SparklineProps> = ({
  values,
  width = 64,
  height = 20,
  min,
  max,
  strokeWidth = 1.5,
  ariaLabel,
  className,
}) => {
  const pad = 3;
  let points: Array<[number, number]> = [];

  if (values.length >= 2) {
    const lo = min ?? Math.min(...values);
    const hi = max ?? Math.max(...values);
    const span = hi - lo || 1;
    const stepX = (width - pad * 2) / (values.length - 1);
    points = values.map((v, i) => [
      pad + i * stepX,
      height - pad - ((v - lo) / span) * (height - pad * 2),
    ]);
  }

  const last = points[points.length - 1];

  return (
    <svg
      role="img"
      aria-label={ariaLabel}
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      className={cn('block shrink-0', className)}
    >
      <line
        x1={0}
        y1={height - 0.5}
        x2={width}
        y2={height - 0.5}
        strokeWidth={1}
        className="stroke-border"
      />
      {last && (
        <>
          <polyline
            fill="none"
            stroke="currentColor"
            strokeWidth={strokeWidth}
            strokeLinecap="round"
            strokeLinejoin="round"
            points={points.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join(' ')}
          />
          <circle cx={last[0]} cy={last[1]} r={2} fill="currentColor" />
        </>
      )}
    </svg>
  );
};
