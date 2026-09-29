import { FC } from 'react';
import { List, Plus, Search } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { IconButton } from '../ui/IconButton';
import { useRemoteState } from '../../services/remoteRegistry';
import { cn } from '../../utils/cn';

interface ConversationRailProps {
  onNewConversation: () => void;
  /** Show the list and put the cursor in its search. */
  onSearch: () => void;
  listOpen: boolean;
  /** Whether the list can be folded; without it there is no list button. */
  canFold: boolean;
  onToggleList: () => void;
  /** The id of the list the list button shows and hides. */
  listId: string;
  /** A chat whose model is on the other machine. */
  remote: boolean;
}

/** Which machine answers, and how this page reaches it, in words. */
function useMachine(remote: boolean): { name: string; how: string; tone: 'ok' | 'away' | 'off'; title: string } {
  const connection = useRemoteState().status?.connected ?? null;
  if (!remote) {
    return { name: 'This machine', how: 'local', tone: 'ok', title: 'The model is served on this machine' };
  }
  if (!connection) {
    return { name: 'Other machine', how: 'not connected', tone: 'off', title: 'The other machine is not connected' };
  }
  if (connection.away_for_s != null) {
    return {
      name: 'Other machine',
      how: `away ${connection.away_for_s}s`,
      tone: 'away',
      title: `The other machine has been away for ${connection.away_for_s} seconds`,
    };
  }
  const how = connection.path === 'relayed' ? 'relayed' : connection.path === 'direct' ? 'direct' : connection.path;
  return { name: 'Other machine', how, tone: 'ok', title: `The other machine, reached ${how}` };
}

const TONE: Record<'ok' | 'away' | 'off', string> = {
  ok: 'bg-success',
  away: 'bg-warning',
  off: 'bg-offline',
};

/**
 * The narrow rail beside the notebook: a new chat, search, the conversation
 * list's fold, and at the foot the machine that answers.
 */
export const ConversationRail: FC<ConversationRailProps> = ({
  onNewConversation,
  onSearch,
  listOpen,
  canFold,
  onToggleList,
  listId,
  remote,
}) => {
  const machine = useMachine(remote);
  return (
    <nav
      aria-label="Chat"
      className="w-[72px] shrink-0 flex flex-col items-center gap-sm py-md bg-background-elevated border-r border-border-light"
    >
      <IconButton label="New chat" variant="outline" size="lg" onClick={onNewConversation}>
        <Icon icon={Plus} size={16} />
      </IconButton>
      <IconButton label="Search conversations" size="lg" onClick={onSearch}>
        <Icon icon={Search} size={16} />
      </IconButton>
      {canFold && (
        <IconButton
          label="Conversations"
          size="lg"
          aria-expanded={listOpen}
          aria-controls={listId}
          className={cn(listOpen && 'bg-surface-elevated text-text')}
          onClick={onToggleList}
        >
          <Icon icon={List} size={16} />
        </IconButton>
      )}
      <div className="flex-1" />
      <div
        className="flex flex-col items-center gap-xs px-xs text-center text-2xs text-text-secondary"
        title={machine.title}
      >
        <span aria-hidden className={cn('w-[7px] h-[7px] rounded-full', TONE[machine.tone])} />
        <span className="leading-tight">{machine.name}</span>
        <span className="font-mono tabular-nums">{machine.how}</span>
      </div>
    </nav>
  );
};
