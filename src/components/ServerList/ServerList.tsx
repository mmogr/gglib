import { FC, useState } from 'react';
import { RotateCcw, ServerOff, Square } from 'lucide-react';
import { ServerInfo } from '../../types';
import { ChatPageTabId, CHAT_PAGE_TABS } from '../../pages/chatTabs';
import SidebarTabs from '../ModelLibraryPanel/SidebarTabs';
import { ServerHealthIndicator } from '../ServerHealthIndicator';
import { Icon } from '../ui/Icon';
import './ServerList.css';

interface ServerListProps {
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onSelectModel?: (modelId: number, view?: 'chat' | 'console') => void;
  /** Compact mode for popover display */
  compact?: boolean;
  /** Show header with count and refresh button */
  showHeader?: boolean;
  onRefresh?: () => void;
}

const ServerList: FC<ServerListProps> = ({
  servers,
  onStopServer,
  onSelectModel,
  compact = false,
  showHeader = false,
  onRefresh,
}) => {
  // Track which server has expanded tabs (only one at a time)
  const [expandedServerId, setExpandedServerId] = useState<number | null>(null);

  const handleStop = async (modelId: number, e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent triggering onSelectModel
    try {
      await onStopServer(modelId);
    } catch (error) {
      console.error('Failed to stop server:', error);
    }
  };

  const handleServerClick = (modelId: number) => {
    // Toggle expanded state for this server
    setExpandedServerId(prev => prev === modelId ? null : modelId);
  };

  const handleTabSelect = (modelId: number, tab: ChatPageTabId) => {
    onSelectModel?.(modelId, tab);
  };

  if (servers.length === 0) {
    return (
      <div className={`server-list-empty ${compact ? 'compact' : ''}`}>
        <div className="empty-icon" aria-hidden>
          <Icon icon={ServerOff} size={22} />
        </div>
        <p>No active servers</p>
        {!compact && <p className="text-muted-small">Start a model to see it here</p>}
      </div>
    );
  }

  return (
    <div className={`server-list ${compact ? 'compact' : ''}`}>
      {showHeader && (
        <div className="server-list-header">
          <span className="server-list-title">
            Active Servers ({servers.length})
          </span>
          {onRefresh && (
            <button
              className="icon-btn icon-btn-sm"
              onClick={onRefresh}
              title="Refresh servers"
            >
              <Icon icon={RotateCcw} size={14} />
            </button>
          )}
        </div>
      )}

      <div className="server-list-items">
        {servers.map((server) => (
          <div
            key={server.model_id}
            className={`server-item ${expandedServerId === server.model_id ? 'expanded' : ''}`}
          >
            <div 
              className={`server-item-header ${onSelectModel ? 'clickable' : ''}`}
              onClick={() => handleServerClick(server.model_id)}
            >
              <div className="server-info">
                <div className="server-name" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                  {server.model_name}
                  <ServerHealthIndicator modelId={server.model_id} />
                </div>
                <div className="server-details">
                  <span className="server-port">:{server.port}</span>
                  <span className="server-status">{server.status}</span>
                </div>
              </div>
              <button
                className={`server-stop-btn ${compact ? 'compact' : ''}`}
                onClick={(e) => handleStop(server.model_id, e)}
                title="Stop server"
              >
                <span className="inline-flex items-center gap-2">
                  <Icon icon={Square} size={14} />
                  {!compact && 'Stop'}
                </span>
              </button>
            </div>
            {expandedServerId === server.model_id && onSelectModel && (
              <div className="server-item-tabs">
                <SidebarTabs<ChatPageTabId>
                  tabs={CHAT_PAGE_TABS}
                  activeTab="chat"
                  onTabChange={(tab) => handleTabSelect(server.model_id, tab)}
                />
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default ServerList;
