import { FC } from 'react';
import { GgufModel, ServerInfo } from '../../types';
import { formatParamCount } from '../../utils/format';
import './ModelLibraryPanel.css';

interface ModelLibraryPanelProps {
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
  onShowWorkPanel?: (tab: 'add-download', subtab?: 'add' | 'download') => void;
}

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
  onShowWorkPanel,
}) => {
  const isModelRunning = (modelId?: number) => {
    if (!modelId) return false;
    return servers.some(s => s.model_id === modelId);
  };

  const toggleTagFilter = (tag: string) => {
    if (selectedTags.includes(tag)) {
      onTagFilterChange(selectedTags.filter(t => t !== tag));
    } else {
      onTagFilterChange([...selectedTags, tag]);
    }
  };

  if (error) {
    return (
      <div className="mcc-panel library-panel">
        <div className="mcc-panel-header">
          <h2>Models Library</h2>
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

  return (
    <div className="mcc-panel library-panel">
      <div className="mcc-panel-header">
        <div className="library-header">
          <h2>Your Models ({models.length})</h2>
          <button 
            onClick={onRefresh} 
            className="icon-btn icon-btn-sm refresh-button" 
            disabled={loading}
            title="Refresh models"
          >
            🔄
          </button>
        </div>

        {/* Search Bar */}
        <div className="search-bar">
          <input
            type="text"
            placeholder="Search models..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            className="form-input form-input-sm search-input"
          />
        </div>

        {/* Tag Filters */}
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
      </div>

      <div className="mcc-panel-content">
        {loading && models.length === 0 ? (
          <div className="loading-state">Loading models...</div>
        ) : models.length === 0 ? (
          <div className="empty-state">
            <div className="empty-icon">📦</div>
            <h3>No models yet</h3>
            <p>Add your first model to get started!</p>
            <div className="empty-actions">
              <button 
                className="btn btn-primary" 
                onClick={() => onShowWorkPanel?.('add-download', 'add')}
              >
                📁 Add from file
              </button>
              <button 
                className="btn btn-secondary"
                onClick={() => onShowWorkPanel?.('add-download', 'download')}
              >
                ⬇️ Download from HF
              </button>
            </div>
          </div>
        ) : (
          <div className="model-table">
            {models.map((model) => (
              <div
                key={model.id || model.name}
                className={`model-row ${selectedModelId === model.id ? 'selected' : ''} ${isModelRunning(model.id) ? 'running' : ''}`}
                onClick={() => onSelectModel(model.id!)}
              >
                <div className="model-row-main">
                  <div className="model-name">
                    {model.name}
                    {isModelRunning(model.id) && (
                      <span className="status-badge running">Running</span>
                    )}
                  </div>
                  <div className="model-metadata">
                    <span className="metadata-item">{formatParamCount(model.param_count_b)}</span>
                    {model.architecture && (
                      <span className="metadata-item">{model.architecture}</span>
                    )}
                    {model.quantization && (
                      <span className="quantization-badge">{model.quantization}</span>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default ModelLibraryPanel;
