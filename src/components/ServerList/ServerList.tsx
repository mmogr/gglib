import { FC, useState } from 'react';
import { ServerInfo } from '../../types';
import SidebarTabs, { SidebarTab } from '../ModelLibraryPanel/SidebarTabs';
import './ServerList.css';

type ServerViewTab = 'chat' | 'console';

const SERVER_VIEW_TABS: SidebarTab<ServerViewTab>[] = [
  { id: 'chat', label: 'Chat', icon: '💬' },
  { id: 'console', label: 'Console', icon: '📟' },
];

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

  const handleTabSelect = (modelId: number, tab: ServerViewTab) => {
    onSelectModel?.(modelId, tab);
  };

  if (servers.length === 0) {
    return (
      <div className={`server-list-empty ${compact ? 'compact' : ''}`}>
        <div className="empty-icon">💤</div>
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
              🔄
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
                <div className="server-name">{server.model_name}</div>
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
                ⏹️{!compact && ' Stop'}
              </button>
            </div>
            {expandedServerId === server.model_id && onSelectModel && (
              <div className="server-item-tabs">
                <SidebarTabs<ServerViewTab>
                  tabs={SERVER_VIEW_TABS}
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
