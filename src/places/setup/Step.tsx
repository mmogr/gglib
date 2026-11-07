/**
 * What every setup step is drawn in: "2 of 5" in 12 px, the 22 px headline,
 * the step's items, and its buttons under them, Back at the left. Also the
 * one item line the steps share: a name, its state, and what is said under
 * it (an error, with its Retry).
 *
 * @module places/setup/Step
 */

import { useEffect, useRef, type FC, type ReactNode } from 'react';
import { Button } from '../../components/ui/Button';

/** How many steps setup has. */
export const STEPS = 5;

/** How long "Already done" is shown before the step moves on by itself. */
export const ADVANCE_MS = 1200;

interface StepProps {
  n: number;
  title: string;
  children: ReactNode;
  /** The buttons at the right, the one ink button last. */
  actions: ReactNode;
  onBack?: () => void;
}

export const Step: FC<StepProps> = ({ n, title, children, actions, onBack }) => (
  <section aria-label={title} className="flex flex-col gap-lg">
    <div className="flex flex-col gap-xs">
      <p className="m-0 text-xs text-text-secondary tabular-nums">{`${n} of ${STEPS}`}</p>
      <h1 className="m-0 text-lg font-semibold text-text">{title}</h1>
    </div>
    <div className="flex flex-col gap-md">{children}</div>
    <div className="flex items-center gap-sm">
      {onBack && (
        <Button variant="ghost" size="sm" onClick={onBack}>
          Back
        </Button>
      )}
      <span className="flex-1" />
      {actions}
    </div>
  </section>
);

interface ItemProps {
  name: string;
  /** The state: a mark and its words. */
  state?: ReactNode;
  /** A control at the end of the line. */
  action?: ReactNode;
  /** What is said under the line. */
  children?: ReactNode;
}

export const Item: FC<ItemProps> = ({ name, state, action, children }) => (
  <div role="group" aria-label={name} className="flex flex-col gap-xs">
    <div className="flex min-h-[36px] flex-wrap items-center gap-x-md gap-y-xs text-sm">
      <span className="w-40 shrink-0 text-text">{name}</span>
      <span className="flex min-w-0 flex-1 flex-wrap items-center gap-sm text-text-secondary">{state}</span>
      {action}
    </div>
    {children && <div className="flex flex-col gap-xs pl-[172px] text-xs text-text-secondary">{children}</div>}
  </div>
);

/**
 * A line under an item with its Retry: an error, said as it came; a
 * `warning` (what setup goes on without) is a status, not an alert.
 */
export const Problem: FC<{ children: ReactNode; onRetry?: () => void; warning?: boolean }> = ({
  children,
  onRetry,
  warning = false,
}) => (
  <p role={warning ? 'status' : 'alert'} className="m-0 flex flex-wrap items-baseline gap-sm text-xs text-text-secondary">
    <span className="break-words">{children}</span>
    {onRetry && (
      <Button variant="link" size="sm" className="text-xs font-normal" onClick={onRetry}>
        Retry
      </Button>
    )}
  </p>
);

/**
 * Say "Already done" for a moment, then move on, while `done` holds and
 * `advance` is given; `null` once it has moved on, so Back to it stays.
 */
export function useAdvance(done: boolean, advance: (() => void) | null): void {
  const latest = useRef(advance);
  useEffect(() => {
    latest.current = advance;
  });
  const due = done && advance !== null;
  useEffect(() => {
    if (!due) return undefined;
    const timer = setTimeout(() => latest.current?.(), ADVANCE_MS);
    return () => clearTimeout(timer);
  }, [due]);
}
