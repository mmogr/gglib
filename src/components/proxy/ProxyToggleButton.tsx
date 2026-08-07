import type { FC } from 'react';
import { Power } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';

interface ProxyToggleButtonProps {
  running: boolean;
  /** Disables the button and switches the label to its -ing form. */
  pending: boolean;
  onStart: () => void;
  onStop: () => void;
  className?: string;
}

const LABELS = {
  start: { idle: 'Start Proxy', pending: 'Starting…' },
  stop: { idle: 'Stop Proxy', pending: 'Stopping…' },
} as const;

/**
 * Start/stop control for the proxy.
 *
 * One component rather than two call sites picking their own variant, so the
 * destructive styling always tracks the destructive action.
 */
export const ProxyToggleButton: FC<ProxyToggleButtonProps> = ({
  running,
  pending,
  onStart,
  onStop,
  className = 'w-full p-md rounded-md text-sm font-semibold',
}) => {
  const labels = running ? LABELS.stop : LABELS.start;

  return (
    <Button
      variant={running ? 'dangerGhost' : 'primary'}
      className={className}
      onClick={running ? onStop : onStart}
      disabled={pending}
      leftIcon={<Icon icon={Power} size={14} />}
    >
      {pending ? labels.pending : labels.idle}
    </Button>
  );
};
