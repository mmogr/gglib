import { FC } from 'react';
import { Box, Plus } from 'lucide-react';
import { GgufModel, ServerInfo } from '../../types';
import { formatParamCount } from '../../utils/format';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { cn } from '../../utils/cn';

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
    return <div className="flex items-center justify-center p-3xl text-text-muted">Loading models...</div>;
  }

  if (models.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-3xl px-xl text-center min-h-[300px]">
        <div className="text-4xl mb-base opacity-50 text-text-disabled" aria-hidden>
          <Icon icon={Box} size={20} />
        </div>
        <h3 className="m-0 mb-sm text-xl font-semibold">No models yet</h3>
        <p className="m-0 mb-lg text-text-secondary">Add your first model to get started!</p>
        <div className="flex flex-wrap justify-center gap-base">
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
    <>
      <div className="flex flex-col w-full">
        {models.map((model) => {
          const isSelected = selectedModelId === model.id;
          const isRunning = isModelRunning(model.id);
          return (
          <div
            key={model.id || model.name}
            className={cn(
              "py-md px-base border-b border-border cursor-pointer transition duration-200 w-full hover:bg-background-hover",
              isSelected && !isRunning && "bg-[rgba(59,130,246,0.2)] border-l-[3px] border-l-primary",
              isRunning && !isSelected && "border-l-[3px] border-l-success",
              isRunning && isSelected && "border-l-[3px] border-l-primary bg-[linear-gradient(90deg,rgba(59,130,246,0.2)_0%,rgba(59,130,246,0.15)_100%)]"
            )}
            onClick={() => onSelectModel(model.id!)}
          >
            <div className="flex flex-col gap-sm w-full">
              <div className="font-medium text-base flex items-center gap-sm w-full break-words">
                {model.name}
                {isRunning && (
                  <span className="py-xs px-sm rounded-md text-xs font-medium bg-success text-text-inverse">Running</span>
                )}
              </div>
              <div className="flex items-center gap-md text-sm text-text-muted flex-wrap">
                <span className="inline-flex items-center">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
                {model.architecture && (
                  <span className="inline-flex items-center">{model.architecture}</span>
                )}
                {model.quantization && (
                  <span className="py-xs px-sm bg-background rounded-sm text-xs font-medium text-primary border border-[rgba(59,130,246,0.3)]">{model.quantization}</span>
                )}
              </div>
            </div>
          </div>
          );
        })}
      </div>
    </>
  );
};

export default ModelsListContent;
