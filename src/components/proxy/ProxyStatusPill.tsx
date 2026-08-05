import type { FC } from 'react';
import { cn } from '../../utils/cn';

interface ProxyStatusPillProps {
  running: boolean;
  className?: string;
}

/**
 * Running/stopped badge for the proxy.
 *
 * Stopped is idle, not an error — danger red is reserved for failures, so a
 * stopped proxy uses the neutral offline colour. See `--color-offline` in
 * `styles/base/variables.css`.
 */
export const ProxyStatusPill: FC<ProxyStatusPillProps> = ({ running, className }) => (
  <span
    className={cn(
      'px-md py-xs rounded-lg text-xs font-semibold uppercase tracking-wider',
      running ? 'bg-success-subtle text-success' : 'bg-background-hover text-offline',
      className,
    )}
  >
    {running ? 'Running' : 'Stopped'}
  </span>
);
