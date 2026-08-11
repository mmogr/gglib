import type { FC } from 'react';
import { cn } from '../../utils/cn';

interface ProxyStatusPillProps {
  running: boolean;
  className?: string;
}

/**
 * Running/stopped indicator for the proxy: status dot + neutral text, per the
 * contract's green-is-a-dot-never-a-fill rule.
 *
 * Stopped is idle, not an error — danger red is reserved for failures, so a
 * stopped proxy uses the neutral offline colour. See `--color-offline` in
 * `styles/base/variables.css`.
 */
export const ProxyStatusPill: FC<ProxyStatusPillProps> = ({ running, className }) => (
  <span className={cn('inline-flex items-center gap-xs text-xs text-text-muted', className)}>
    <span
      aria-hidden
      className={cn('w-1.5 h-1.5 rounded-full', running ? 'bg-success' : 'bg-offline')}
    />
    {running ? 'Running' : 'Stopped'}
  </span>
);
