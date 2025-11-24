import { FC } from 'react';
import './RunsTab.css';

interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  status: string;
}

interface RunsTabProps {
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onRefresh: () => void;
  onSelectModel: (id: number) => void;
}

const RunsTab: FC<RunsTabProps> = ({
  servers,
  onStopServer,
  onRefresh,
  onSelectModel,
}) => {
  const handleStop = async (modelId: number) => {
    try {
      await onStopServer(modelId);
    } catch (error) {
      console.error('Failed to stop server:', error);
      alert(`Failed to stop server: ${error}`);
    }
  };

  if (servers.length === 0) {
    return (
      <div className="runs-empty">
        <div className="empty-icon">💤</div>
        <p>No active servers</p>
        <p className="text-muted-small">Start a model endpoint to see it here</p>
      </div>
    );
  }

  return (
    <div className="runs-tab">
      <div className="runs-header">
        <h3>Active Servers ({servers.length})</h3>
        <button
          className="icon-btn icon-btn-sm"
          onClick={onRefresh}
          title="Refresh servers"
        >
          🔄
        </button>
      </div>

      <div className="runs-list">
        {servers.map((server) => {
          return (
            <div key={server.model_id} className="run-item">
              <div
                className="run-main"
                onClick={() => onSelectModel(server.model_id)}
                style={{ cursor: 'pointer' }}
              >
                <div className="run-name">{server.model_name}</div>
                <div className="run-details">
                  <span className="run-port">Port: {server.port}</span>
                  <span className="run-status">{server.status}</span>
                </div>
              </div>
              <div className="run-actions">
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={() => handleStop(server.model_id)}
                  title="Stop server"
                >
                  ⏹️ Stop
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default RunsTab;
