import { FC } from 'react';
import { Button } from '../ui/Button';
import type { ChatSource, RemoteConnection } from '../../services/transport';
import { cn } from '../../utils/cn';

interface SourceSwitchProps {
  source: ChatSource;
  onSource: (source: ChatSource) => void;
  /** The joined machine, while it is connected. */
  connection: RemoteConnection | null;
}

/** How the far machine is reached, in a word or two. */
function reached(connection: RemoteConnection | null): string {
  if (!connection) return 'not connected';
  if (connection.away_for_s != null) return `away ${connection.away_for_s}s`;
  return connection.path;
}

/**
 * The rail's foot on a joined computer: whose chats the list shows, this
 * machine's or the far machine's, which it reads through the tunnel and
 * never copies.
 */
export const SourceSwitch: FC<SourceSwitchProps> = ({ source, onSource, connection }) => {
  const choices: Array<{ id: ChatSource; name: string; how: string; title: string }> = [
    { id: 'this', name: 'This machine', how: 'local', title: 'The chats kept on this machine' },
    {
      id: 'far',
      name: 'Other machine',
      how: reached(connection),
      title: connection
        ? `The chats kept on the other machine (${connection.ticket_fingerprint}), read through the tunnel`
        : 'The other machine is not connected',
    },
  ];
  return (
    <div role="group" aria-label="Whose chats" className="flex flex-col items-stretch gap-xs w-full px-xs">
      {choices.map((choice) => (
        <Button
          key={choice.id}
          variant="ghost"
          size="sm"
          aria-pressed={source === choice.id}
          title={choice.title}
          onClick={() => onSource(choice.id)}
          className={cn(
            'h-auto flex-col gap-[2px] px-xs py-xs text-2xs leading-tight whitespace-normal',
            source === choice.id && 'bg-surface-elevated text-text',
          )}
        >
          <span>{choice.name}</span>
          <span className="font-mono tabular-nums text-text-muted">{choice.how}</span>
        </Button>
      ))}
    </div>
  );
};
