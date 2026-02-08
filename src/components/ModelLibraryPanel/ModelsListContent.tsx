import { FC } from 'react';
import { Box, Plus } from 'lucide-react';
import { GgufModel, ServerInfo } from '../../types';
import { formatParamCount } from '../../utils/format';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';

interface ModelsListContentProps {
  models: GgufModel[];
  selectedModelId: number | null;
  onSelectModel: (id: number | null) => void;
  loading: boolean;
  servers: ServerInfo[];
  onSwitchToAddTab?: () => void;
}

const ModelsListContent: FC<ModelsListContentProps> = ({
  models,
  selectedModelId,
  onSelectModel,
  loading,
  servers,
  onSwitchToAddTab,
}) => {
  const isModelRunning = (modelId?: number) => {
    if (!modelId) return false;
    return servers.some(s => s.modelId === modelId);
  };

  if (loading && models.length === 0) {
    return <div className="loading-state">Loading models...</div>;
  }

  if (models.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-icon" aria-hidden>
          <Icon icon={Box} size={20} />
        </div>
        <h3>No models yet</h3>
        <p>Add your first model to get started!</p>
        <div className="empty-actions">
          <Button 
            variant="primary" 
            onClick={onSwitchToAddTab}
            leftIcon={<Icon icon={Plus} size={14} />}
          >
            Add Models
          </Button>
        </div>
      </div>
    );
  }

  return (
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
              <span className="metadata-item">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
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
  );
};

export default ModelsListContent;
