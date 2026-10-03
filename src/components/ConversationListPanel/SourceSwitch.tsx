import { FC } from 'react';
import { Button } from '../ui/Button';
import type { ChatSource, RemoteConnection } from '../../services/transport';
import { cn } from '../../utils/cn';

interface SourceSwitchProps {
  source: ChatSource;
  onSource: (source: ChatSource) => void;
  /** The joined machine, while it is connected. */
  connection: RemoteConnection | null;
  /**
   * The name the joined machine is shown by; never its fingerprint. A host
   * label can be one long word, so it may break anywhere to stay in the rail.
   */
  machine: string;
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
export const SourceSwitch: FC<SourceSwitchProps> = ({ source, onSource, connection, machine }) => {
  const choices: Array<{ id: ChatSource; name: string; how: string; title: string }> = [
    { id: 'this', name: 'This machine', how: 'local', title: 'The chats kept on this machine' },
    {
      id: 'far',
      name: `${machine}'s chats`,
      how: reached(connection),
      title: connection
        ? `The chats kept on ${machine}, read through the tunnel`
        : `${machine} is not connected`,
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
            'h-auto px-xs py-xs text-2xs leading-tight',
            source === choice.id && 'bg-surface-elevated text-text',
          )}
        >
          {/* One child, because Button lays its children in a row: the name
              sits over how the machine is reached in a 56px column. */}
          <span className="flex flex-col items-center gap-[2px] min-w-0 text-center">
            <span className="wrap-anywhere">{choice.name}</span>
            <span className="font-mono tabular-nums text-text-muted">{choice.how}</span>
          </span>
        </Button>
      ))}
    </div>
  );
};
