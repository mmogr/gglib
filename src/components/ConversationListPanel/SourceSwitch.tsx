import { FC } from 'react';
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
        // Not the shared Button: it lays its children in a row, and here the
        // name sits over how the machine is reached in a 56px column.
        <button
          key={choice.id}
          type="button"
          aria-pressed={source === choice.id}
          title={choice.title}
          onClick={() => onSource(choice.id)}
          className={cn(
            'flex flex-col items-center gap-[2px] w-full rounded-base px-xs py-xs text-center text-2xs leading-tight',
            'text-text-secondary cursor-pointer transition-colors duration-200 hover:text-text hover:bg-surface-elevated',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-2 focus-visible:ring-offset-surface',
            source === choice.id && 'bg-surface-elevated text-text',
          )}
        >
          <span className="wrap-anywhere">{choice.name}</span>
          <span className="font-mono tabular-nums text-text-muted">{choice.how}</span>
        </button>
      ))}
    </div>
  );
};
