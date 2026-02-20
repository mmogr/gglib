import { FC } from 'react';
import { Plus, X } from 'lucide-react';
import type { ConversationSummary } from '../../services/clients/chat';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { cn } from '../../utils/cn';

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
    <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden border-r border-border relative flex-1 bg-surface max-md:h-auto max-md:max-h-none max-md:border-r-0 max-md:border-b max-md:border-border">
      <div className="p-base border-b border-border bg-background shrink-0">
        {/* View Tabs */}
        <div className="mb-md">
          <SidebarTabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeTab={activeTab}
            onTabChange={onTabChange}
          />
        </div>

        <div className="flex justify-between items-start gap-md max-mobile:flex-col max-mobile:gap-sm">
          <div className="flex flex-col gap-xs min-w-0">
            <span className="text-xs uppercase tracking-[1px] text-text-muted">Chatting with</span>
            <h2 className="text-lg font-semibold m-0 text-text overflow-hidden text-ellipsis whitespace-nowrap">{modelName}</h2>
          </div>
          <div className="flex gap-sm items-center shrink-0 max-mobile:w-full max-mobile:justify-between">
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
              variant="danger"
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
          <div className="flex flex-col gap-sm">
            {filteredConversations.map((conversation) => (
              <button
                key={conversation.id}
                type="button"
                className={cn(
                  "group/item flex justify-between items-center py-md px-base border border-border rounded-base bg-transparent text-inherit text-left cursor-pointer transition-all duration-200 hover:border-primary hover:bg-background-hover",
                  conversation.id === activeConversationId && "border-primary bg-primary/10"
                )}
                onClick={() => onSelectConversation(conversation.id)}
              >
                <div className="flex flex-col gap-xs min-w-0 flex-1">
                  <span className="font-medium text-text overflow-hidden text-ellipsis whitespace-nowrap">{conversation.title}</span>
                  <span className="text-sm text-text-muted">
                    {formatRelativeTime(conversation.updated_at)}
                  </span>
                </div>
                <button
                  type="button"
                  className="opacity-0 group-hover/item:opacity-100 border-0 bg-transparent text-text-muted cursor-pointer p-xs rounded-sm transition-all duration-200 shrink-0 hover:bg-danger/10 hover:text-danger"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteConversation(conversation.id);
                  }}
                  title="Delete conversation"
                >
                  <Icon icon={X} size={12} />
                </button>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ConversationListPanel;
