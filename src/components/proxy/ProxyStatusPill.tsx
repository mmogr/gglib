import type { FC } from 'react';
import { cn } from '../../utils/cn';

interface ProxyStatusPillProps {
  running: boolean;
  /**
   * Whether the daemon answered at all. `false` overrides `running`, since a
   * proxy state read from a daemon that is not there is not a reading.
   * Omit where a daemon is a given — the main window cannot open without one.
   */
  daemonReachable?: boolean;
  className?: string;
}

/**
 * Running/stopped indicator for the proxy: status dot + neutral text, per the
 * contract's green-is-a-dot-never-a-fill rule.
 *
 * Stopped is idle, not an error — danger red is reserved for failures, so a
 * stopped proxy uses the neutral offline colour. See `--color-offline` in
 * `styles/base/variables.css`.
 *
 * "No service" is a third state, not a stopped proxy. They rendered
 * identically until the tray popover learned to tell them apart, which made a
 * machine with no daemon look like one with an idle endpoint — and the
 * difference is the whole question of whether Start is going to work.
 */
export const ProxyStatusPill: FC<ProxyStatusPillProps> = ({
  running,
  daemonReachable = true,
  className,
}) => {
  const label = !daemonReachable ? 'No service' : running ? 'Running' : 'Stopped';

  return (
    <span className={cn('inline-flex items-center gap-xs text-xs text-text-muted', className)}>
      <span
        aria-hidden
        className={cn(
          'w-1.5 h-1.5 rounded-full',
          running && daemonReachable ? 'bg-success' : 'bg-offline',
        )}
      />
      {label}
    </span>
  );
};
