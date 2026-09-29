import { FC, Ref } from 'react';
import { X } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { IconButton } from '../ui/IconButton';
import { Input } from '../ui/Input';
import { Stack } from '../primitives';
import { cn } from '../../utils/cn';
import { EmptyState } from '../primitives';
import { ConversationListSkeleton } from './ConversationListSkeleton';
import { ConversationMarks } from './ConversationMarks';
import type { ConversationSummary } from '../../services/transport';

interface ConversationListPanelProps {
  conversations: ConversationSummary[];
  activeConversationId: number | null;
  onSelectConversation: (id: number) => void;
  onDeleteConversation: (id: number) => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  loading: boolean;
  /** The search field, for the rail's search button to focus. */
  searchInputRef?: Ref<HTMLInputElement>;
  /** Conversations with a reply running. */
  running?: ReadonlySet<number>;
  /** Conversations with a reply not yet seen. */
  unread?: ReadonlySet<number>;
}

const NONE: ReadonlySet<number> = new Set();

const formatRelativeTime = (iso: string) => {
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  const date = new Date(iso);
  const diffMinutes = Math.round((date.getTime() - Date.now()) / (1000 * 60));

  if (Math.abs(diffMinutes) < 60) {
    return formatter.format(diffMinutes, 'minute');
  }

  const diffHours = Math.round(diffMinutes / 60);
  if (Math.abs(diffHours) < 24) {
    return formatter.format(diffHours, 'hour');
  }

  const diffDays = Math.round(diffHours / 24);
  return formatter.format(diffDays, 'day');
};

const ConversationListPanel: FC<ConversationListPanelProps> = ({
  conversations,
  activeConversationId,
  onSelectConversation,
  onDeleteConversation,
  searchQuery,
  onSearchChange,
  loading,
  searchInputRef,
  running = NONE,
  unread = NONE,
}) => {
  const filteredConversations = searchQuery.trim()
    ? conversations.filter(c => 
        c.title.toLowerCase().includes(searchQuery.trim().toLowerCase())
      )
    : conversations;

  return (
    <div className="flex flex-col overflow-hidden relative flex-1 min-h-0 bg-background-elevated">
      <div className="p-md border-b border-border-light shrink-0 flex flex-col gap-sm">
        <h2 className="text-sm font-semibold m-0 text-text-secondary">Conversations</h2>
        <Input
          ref={searchInputRef}
          type="search"
          aria-label="Search conversations"
          placeholder="Search conversations..."
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          className="w-full"
          size="sm"
        />
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        {loading ? (
          <ConversationListSkeleton />
        ) : filteredConversations.length === 0 ? (
          <EmptyState
            className="p-xl"
            title={searchQuery ? 'No matching conversations' : 'No conversations yet'}
            description={
              searchQuery
                ? 'Try a different search term.'
                : 'Send a message to start your first conversation.'
            }
          />
        ) : (
          <div role="listbox" aria-label="Conversations" className="flex flex-col">
            {filteredConversations.map((conversation) => (
              // A div, not a button: the delete control is nested inside, and
              // interactive elements must not contain other interactive elements.
              // Same no-layout-shift trick as the model list rows: the accent
              // border is always present, transparent when idle.
              <div
                key={conversation.id}
                role="option"
                aria-selected={conversation.id === activeConversationId}
                tabIndex={0}
                className={cn(
                  "group/item flex justify-between items-center gap-sm py-sm px-md border-l-[3px] border-l-transparent text-left cursor-pointer transition-colors hover:bg-background-hover",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-primary",
                  conversation.id === activeConversationId && "border-l-primary bg-primary-subtle"
                )}
                onClick={() => onSelectConversation(conversation.id)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    onSelectConversation(conversation.id);
                  }
                }}
              >
                <Stack gap="xs" className="min-w-0 flex-1">
                  <span
                    className="font-medium text-sm text-text overflow-hidden text-ellipsis whitespace-nowrap"
                    title={conversation.title}
                  >
                    {conversation.title}
                  </span>
                  <span className="flex flex-wrap items-center gap-sm text-xs text-text-muted">
                    {formatRelativeTime(conversation.updated_at)}
                    <ConversationMarks
                      running={running.has(conversation.id)}
                      unread={unread.has(conversation.id)}
                    />
                  </span>
                </Stack>
                <IconButton
                  label="Delete conversation"
                  size="sm"
                  variant="dangerGhost"
                  className="opacity-0 group-hover/item:opacity-100 focus-visible:opacity-100 shrink-0"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteConversation(conversation.id);
                  }}
                >
                  <Icon icon={X} size={12} />
                </IconButton>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ConversationListPanel;
