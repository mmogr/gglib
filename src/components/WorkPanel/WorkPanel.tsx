import { FC } from 'react';
import { WorkPanelTab } from '../../pages/ModelControlCenterPage';
import { ServerInfo } from '../../types';
import AddDownloadTab from './AddDownloadTab';
import RunsTab from './RunsTab';
import './WorkPanel.css';

interface WorkPanelProps {
  activeTab: WorkPanelTab;
  onTabChange: (tab: WorkPanelTab) => void;
  onModelAdded: (filePath: string) => Promise<void>;
  onModelDownloaded: () => Promise<void>;
  servers: ServerInfo[];
  onStopServer: (modelId: number) => Promise<void>;
  onRefreshServers: () => void;
  onSelectModel: (id: number) => void;
  activeSubTab?: 'add' | 'download';
  onSubTabChange?: (subtab: 'add' | 'download') => void;
}

const WorkPanel: FC<WorkPanelProps> = ({
  activeTab,
  onTabChange,
  onModelAdded,
  onModelDownloaded,
  servers,
  onStopServer,
  onRefreshServers,
  onSelectModel,
  activeSubTab,
  onSubTabChange,
}) => {
  return (
    <div className="mcc-panel work-panel">
      <div className="mcc-panel-header">
        <div className="work-tabs">
          <button
            className={`work-tab ${activeTab === 'add-download' ? 'active' : ''}`}
            onClick={() => onTabChange('add-download')}
          >
            Add / Download
          </button>
          <button
            className={`work-tab ${activeTab === 'runs' ? 'active' : ''}`}
            onClick={() => onTabChange('runs')}
          >
            Runs {servers.length > 0 && <span className="tab-badge">{servers.length}</span>}
          </button>
        </div>
      </div>

      <div className="mcc-panel-content">
        {activeTab === 'add-download' && (
          <AddDownloadTab
            onModelAdded={onModelAdded}
            onModelDownloaded={onModelDownloaded}
            activeSubTab={activeSubTab}
            onSubTabChange={onSubTabChange}
          />
        )}
        {activeTab === 'runs' && (
          <RunsTab
            servers={servers}
            onStopServer={onStopServer}
            onRefresh={onRefreshServers}
            onSelectModel={onSelectModel}
          />
        )}
      </div>
    </div>
  );
};

export default WorkPanel;
