import { FC } from 'react';
import { Box, Plus, Zap } from 'lucide-react';
import { GgufModel, ServerInfo } from '../../types';
import { formatParamCount } from '../../utils/format';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { EmptyState } from '../primitives/EmptyState';
import { cn } from '../../utils/cn';
import { Chip } from '../ui/Chip';

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
      <EmptyState
        className="min-h-[300px]"
        icon={<Icon icon={Box} size={22} />}
        title="No models yet"
        description="Add your first model to get started."
        action={
          <Button
            variant="primary"
            onClick={onSwitchToAddTab}
            leftIcon={<Icon icon={Plus} size={14} />}
          >
            Add Models
          </Button>
        }
      />
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
              "py-sm px-md text-left border-l-[3px] border-l-transparent cursor-pointer transition duration-200 w-full bg-transparent hover:bg-background-hover focus-visible:outline-none focus-visible:bg-background-hover focus-visible:border-l-primary",
              isSelected && "bg-primary-subtle border-l-primary",
              isRunning && !isSelected && "border-l-success",
            )}
            onClick={() => onSelectModel(model.id!)}
          >
            <div className="flex flex-col gap-xs w-full">
              <div className="font-medium text-sm flex items-center gap-sm w-full break-words">
                {model.name}
                {isRunning && (
                  <Chip variant="success" size="sm">Running</Chip>
                )}
              </div>
              <div className="flex items-center gap-md text-xs text-text-muted flex-wrap">
                <span className="inline-flex items-center">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
                {model.architecture && (
                  <span className="inline-flex items-center">{model.architecture}</span>
                )}
                {/* Neutral: quantization and throughput are facts about the
                    model, not states needing attention. */}
                {model.quantization && (
                  <Chip size="sm" className="font-mono">{model.quantization}</Chip>
                )}
                {tps != null && (
                  <Chip size="sm" leftIcon={<Icon icon={Zap} size={11} />} className="tabular-nums">
                    {tps.toFixed(0)} t/s
                  </Chip>
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
