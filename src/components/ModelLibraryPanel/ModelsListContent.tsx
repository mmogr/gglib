import { FC } from 'react';
import { Box, Plus, Zap } from 'lucide-react';
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
      <div className="flex flex-col w-full" role="listbox" aria-label="Model library">
        {models.map((model) => {
          const isSelected = selectedModelId === model.id;
          const isRunning = isModelRunning(model.id);
          const tps = model.benchmarkSummary?.latest_tg_tps ?? model.benchmarkSummary?.best_tg_tps;
          return (
          <button
            key={model.id || model.name}
            type="button"
            role="option"
            aria-selected={isSelected}
            // The accent border is always present but transparent when idle,
            // so selecting a row recolours it instead of shifting the text 3px.
            className={cn(
              "py-md px-base text-left border-b border-border border-l-[3px] border-l-transparent cursor-pointer transition duration-200 w-full bg-transparent hover:bg-background-hover focus-visible:outline-none focus-visible:bg-background-hover focus-visible:border-l-primary",
              isSelected && "bg-primary-subtle border-l-primary",
              isRunning && !isSelected && "border-l-success",
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
                {/* Neutral: quantization and throughput are facts about the
                    model, not states needing attention. */}
                {model.quantization && (
                  <span className="py-xs px-sm bg-background rounded-sm text-xs font-medium text-text-secondary border border-border">{model.quantization}</span>
                )}
                {tps != null && (
                  <span className="inline-flex items-center gap-xs py-xs px-sm bg-background text-text-secondary rounded-sm text-xs font-medium border border-border">
                    <Icon icon={Zap} size={11} />
                    {tps.toFixed(0)} t/s
                  </span>
                )}
              </div>
            </div>
          </button>
          );
        })}
      </div>
    </>
  );
};

export default ModelsListContent;
