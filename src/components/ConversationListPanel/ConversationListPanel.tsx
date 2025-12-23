import { FC } from 'react';
import { Plus, X } from 'lucide-react';
import type { ConversationSummary } from '../../services/clients/chat';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import './ConversationListPanel.css';

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
    <div className="mcc-panel conversation-list-panel">
      <div className="mcc-panel-header">
        {/* View Tabs */}
        <div className="conversation-list-tabs">
          <SidebarTabs<ChatPageTabId>
            tabs={CHAT_PAGE_TABS}
            activeTab={activeTab}
            onTabChange={onTabChange}
          />
        </div>

        <div className="conversation-list-header">
          <div className="conversation-list-title-group">
            <span className="conversation-list-label">Chatting with</span>
            <h2 className="conversation-list-title">{modelName}</h2>
          </div>
          <div className="conversation-list-actions">
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
        
        <div className="search-bar">
          <Input
            type="text"
            placeholder="Search conversations..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="search-input"
            size="sm"
          />
        </div>
      </div>

      <div className="mcc-panel-content">
        {loading ? (
          <div className="conversation-list-empty">Loading conversations…</div>
        ) : filteredConversations.length === 0 ? (
          <div className="conversation-list-empty">
            {searchQuery ? 'No matching conversations' : 'No conversations yet'}
          </div>
        ) : (
          <div className="conversation-list">
            {filteredConversations.map((conversation) => (
              <button
                key={conversation.id}
                type="button"
                className={`conversation-item ${
                  conversation.id === activeConversationId ? 'active' : ''
                }`}
                onClick={() => onSelectConversation(conversation.id)}
              >
                <div className="conversation-item-content">
                  <span className="conversation-item-title">{conversation.title}</span>
                  <span className="conversation-item-time">
                    {formatRelativeTime(conversation.updated_at)}
                  </span>
                </div>
                <button
                  type="button"
                  className="conversation-item-delete"
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
