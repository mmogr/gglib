import { FC, useState } from 'react';
import { GgufModel, ServerInfo } from '../../types';
import SidebarTabs, { SidebarTabId, SidebarTab } from './SidebarTabs';
import ModelsListContent from './ModelsListContent';
import AddDownloadContent, { AddDownloadSubTab } from './AddDownloadContent';
import ProxyControl from '../ProxyControl';
import './ModelLibraryPanel.css';

interface ModelLibraryPanelProps {
  // Models list props
  models: GgufModel[];
  selectedModelId: number | null;
  onSelectModel: (id: number | null) => void;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  tags: string[];
  selectedTags: string[];
  onTagFilterChange: (tags: string[]) => void;
  servers: ServerInfo[];
  
  // Add/Download props
  onModelAdded: (filePath: string) => Promise<void>;
  onModelDownloaded: () => Promise<void>;
  activeSubTab?: AddDownloadSubTab;
  onSubTabChange?: (subtab: AddDownloadSubTab) => void;
  
  // Tab control (optional - can be controlled externally or internally)
  activeTab?: SidebarTabId;
  onTabChange?: (tab: SidebarTabId) => void;
}

const SIDEBAR_TABS: SidebarTab[] = [
  { id: 'models', label: 'Your Models', icon: '📋' },
  { id: 'add', label: 'Add Models', icon: '➕' },
];

const ModelLibraryPanel: FC<ModelLibraryPanelProps> = ({
  models,
  selectedModelId,
  onSelectModel,
  loading,
  error,
  onRefresh,
  searchQuery,
  onSearchChange,
  tags,
  selectedTags,
  onTagFilterChange,
  servers,
  onModelAdded,
  onModelDownloaded,
  activeSubTab,
  onSubTabChange,
  activeTab: externalActiveTab,
  onTabChange: externalOnTabChange,
}) => {
  // Internal tab state (used if not controlled externally)
  const [internalActiveTab, setInternalActiveTab] = useState<SidebarTabId>('models');
  const activeTab = externalActiveTab ?? internalActiveTab;
  
  const handleTabChange = (tab: SidebarTabId) => {
    if (externalOnTabChange) {
      externalOnTabChange(tab);
    } else {
      setInternalActiveTab(tab);
    }
  };

  const toggleTagFilter = (tag: string) => {
    if (selectedTags.includes(tag)) {
      onTagFilterChange(selectedTags.filter(t => t !== tag));
    } else {
      onTagFilterChange([...selectedTags, tag]);
    }
  };

  const handleSwitchToAddTab = () => {
    handleTabChange('add');
  };

  // Error state
  if (error) {
    return (
      <div className="mcc-panel library-panel">
        <div className="mcc-panel-header">
          <SidebarTabs
            tabs={SIDEBAR_TABS}
            activeTab={activeTab}
            onTabChange={handleTabChange}
          />
        </div>
        <div className="mcc-panel-content">
          <div className="error-container">
            <p className="error-message">Error: {error}</p>
            <button onClick={onRefresh} className="retry-button">
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Header actions (refresh + proxy)
  const headerActions = (
    <>
      {activeTab === 'models' && (
        <button 
          onClick={onRefresh} 
          className="icon-btn icon-btn-sm" 
          disabled={loading}
          title="Refresh models"
        >
          🔄
        </button>
      )}
      <ProxyControl
        buttonClassName="icon-btn icon-btn-sm proxy-sidebar-btn"
        buttonActiveClassName="proxy-sidebar-btn-active"
        statusDotClassName="proxy-status-dot"
        statusDotActiveClassName="proxy-status-dot-active"
      />
    </>
  );

  return (
    <div className="mcc-panel library-panel">
      <div className="mcc-panel-header">
        <SidebarTabs
          tabs={SIDEBAR_TABS}
          activeTab={activeTab}
          onTabChange={handleTabChange}
          rightContent={headerActions}
        />

        {/* Search and filters - only show on models tab */}
        {activeTab === 'models' && (
          <>
            <div className="search-bar">
              <input
                type="text"
                placeholder="Search models..."
                value={searchQuery}
                onChange={(e) => onSearchChange(e.target.value)}
                className="form-input form-input-sm search-input"
              />
            </div>

            {tags.length > 0 && (
              <div className="tag-filters">
                {tags.map(tag => (
                  <button
                    key={tag}
                    className={`tag-filter-chip ${selectedTags.includes(tag) ? 'active' : ''}`}
                    onClick={() => toggleTagFilter(tag)}
                  >
                    {tag}
                  </button>
                ))}
              </div>
            )}
          </>
        )}
      </div>

      <div className="mcc-panel-content">
        {activeTab === 'models' ? (
          <ModelsListContent
            models={models}
            selectedModelId={selectedModelId}
            onSelectModel={onSelectModel}
            loading={loading}
            servers={servers}
            onSwitchToAddTab={handleSwitchToAddTab}
          />
        ) : (
          <AddDownloadContent
            onModelAdded={onModelAdded}
            onModelDownloaded={onModelDownloaded}
            activeSubTab={activeSubTab}
            onSubTabChange={onSubTabChange}
          />
        )}
      </div>
    </div>
  );
};

export default ModelLibraryPanel;
