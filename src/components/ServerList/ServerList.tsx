import { FC } from 'react';
import { ServerInfo } from '../../types';
import './ServerList.css';

interface ServerListProps {
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onSelectModel?: (modelId: number) => void;
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
  const handleStop = async (modelId: number, e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent triggering onSelectModel
    try {
      await onStopServer(modelId);
    } catch (error) {
      console.error('Failed to stop server:', error);
    }
  };

  const handleSelect = (modelId: number) => {
    onSelectModel?.(modelId);
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
            className={`server-item ${onSelectModel ? 'clickable' : ''}`}
            onClick={() => handleSelect(server.model_id)}
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
        ))}
      </div>
    </div>
  );
};

export default ServerList;
