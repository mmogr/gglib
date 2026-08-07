import { FC } from 'react';
import { Plus, X } from 'lucide-react';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import { Tabs } from '../ui/Tabs';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { IconButton } from '../ui/IconButton';
import { Input } from '../ui/Input';
import { Stack } from '../primitives';
import { cn } from '../../utils/cn';
import type { ConversationSummary } from '../../services/transport';

interface ConversationListPanelProps {
  conversations: ConversationSummary[];
  activeConversationId: number | null;
  onSelectConversation: (id: number) => void;
  onDeleteConversation: (id: number) => void;
  onNewConversation: () => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  loading: boolean;
  modelName: string;
  onClose: () => void;
  activeTab: ChatPageTabId;
  onTabChange: (tab: ChatPageTabId) => void;
}

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
  onNewConversation,
  searchQuery,
  onSearchChange,
  loading,
  modelName,
  onClose,
  activeTab,
  onTabChange,
}) => {
  const filteredConversations = searchQuery.trim()
    ? conversations.filter(c => 
        c.title.toLowerCase().includes(searchQuery.trim().toLowerCase())
      )
    : conversations;

  return (
    <div className="flex flex-col overflow-hidden border-b border-border relative flex-1 bg-surface md:h-full md:min-h-0 md:border-b-0 md:border-r">
      <div className="p-md border-b border-border-light shrink-0">
        {/* View Tabs */}
        <div className="mb-md">
          <Tabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeId={activeTab}
            onChange={onTabChange}
            aria-label="Chat views"
          />
        </div>

        <div className="flex flex-col gap-sm mobile:flex-row mobile:justify-between mobile:items-start mobile:gap-md">
          <Stack gap="xs" className="min-w-0">
            <span className="text-xs font-medium text-text-muted">Chatting with</span>
            <h2 className="text-lg font-semibold m-0 text-text overflow-hidden text-ellipsis whitespace-nowrap">{modelName}</h2>
          </Stack>
          <div className="flex gap-sm items-center w-full justify-between mobile:w-auto mobile:shrink-0">
            <Button
              variant="primary"
              size="sm"
              onClick={onNewConversation}
              title="New conversation"
              leftIcon={<Icon icon={Plus} size={14} />}
            >
              New
            </Button>
            <Button
              variant="dangerGhost"
              size="sm"
              onClick={onClose}
              title="Stop server and close chat"
              leftIcon={<Icon icon={X} size={14} />}
            >
              Close
            </Button>
          </div>
        </div>
        
        <div className="flex-1">
          <Input
            type="text"
            placeholder="Search conversations..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="w-full"
            size="sm"
          />
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        {loading ? (
          <div className="flex items-center justify-center p-xl text-text-muted text-center">Loading conversations…</div>
        ) : filteredConversations.length === 0 ? (
          <div className="flex items-center justify-center p-xl text-text-muted text-center">
            {searchQuery ? 'No matching conversations' : 'No conversations yet'}
          </div>
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
                  <span className="text-xs text-text-muted">
                    {formatRelativeTime(conversation.updated_at)}
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
