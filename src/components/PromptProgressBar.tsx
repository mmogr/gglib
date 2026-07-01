/**
 * PromptProgressBar.
 *
 * Horizontal Tailwind progress bar for the prompt-processing phase
 * (`prompt_processed`/`prompt_total`) — the same information the CLI renders
 * as a unicode block bar (`progress_bar()` in
 * `crates/gglib-cli/src/handlers/proxy_dashboard.rs`), rendered here as a
 * proper element instead.
 *
 * @module components/PromptProgressBar
 */

import type { FC } from 'react';
import { cn } from '../utils/cn';

export interface PromptProgressBarProps {
  /** Numerator, e.g. tokens processed so far. */
  processed: number | null;
  /** Denominator, e.g. total prompt tokens. */
  total: number | null;
  /** Caption rendered above the bar, e.g. "Processing prompt". */
  label?: string;
  className?: string;
}

export const PromptProgressBar: FC<PromptProgressBarProps> = ({ processed, total, label, className }) => {
  const fraction = processed != null && total ? Math.min(Math.max(processed / total, 0), 1) : 0;
  const pct = Math.round(fraction * 100);

  return (
    <div className={cn('w-full', className)}>
      {label && (
        <div className="flex justify-between text-xs text-text-secondary mb-xs">
          <span>{label}</span>
          <span>
            {processed != null && total ? `${processed}/${total} · ${pct}%` : '—'}
          </span>
        </div>
      )}
      <div
        className="w-full h-2 rounded-full bg-surface-elevated border border-border overflow-hidden"
        role="progressbar"
        aria-valuenow={pct}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className="h-full rounded-full bg-primary transition-[width] duration-300 ease-out"
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
};

export default PromptProgressBar;
