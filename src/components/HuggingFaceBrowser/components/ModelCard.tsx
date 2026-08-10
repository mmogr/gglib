import { FC } from "react";
import { Download, ExternalLink, Heart, Wrench } from "lucide-react";
import { HfModelSummary } from "../../../types";
import { formatNumber, getHuggingFaceModelUrl } from "../../../utils/format";
import { useToolSupportCache } from "../../../hooks/useToolSupportCache";
import { openUrl } from "../../../services/platform";
import { Icon } from "../../ui/Icon";
import { IconButton } from '../../ui/IconButton';
import { cn } from '../../../utils/cn';

export interface ModelCardProps {
  model: HfModelSummary;
  /** Callback when the model card is clicked (for preview) */
  onSelect: () => void;
  /** Whether this model is currently selected */
  isSelected: boolean;
}

/**
 * Simplified model card - displays model info, click to select for preview.
 */
export const ModelCard: FC<ModelCardProps> = ({ 
  model, 
  onSelect,
  isSelected,
}) => {
  // Lazy-load tool support detection (fires immediately, cached across renders)
  const { supports: supportsTools } = useToolSupportCache(model.id);

  const handleOpenHuggingFace = (e: React.MouseEvent) => {
    e.stopPropagation();
    const url = getHuggingFaceModelUrl(model.id);
    openUrl(url);
  };

  return (
    <div
      className={cn(
        'relative bg-surface-elevated rounded-md mb-3 overflow-hidden transition-all duration-200 ease-linear hover:bg-surface-hover',
        isSelected && 'bg-primary-subtle ring-1 ring-primary-border hover:bg-primary/15'
      )}
    >
      {/* Stretched select control: a real button underlaying the card keeps the
          nested HuggingFace button an independent sibling — no interactive
          nesting, and each control keeps its own native keyboard activation. */}
      {/* eslint-disable-next-line no-restricted-syntax -- sr-only stretched select control; Button's chrome would fight the card */}
      <button
        type="button"
        aria-pressed={isSelected}
        onClick={onSelect}
        className="absolute inset-0 w-full cursor-pointer rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
      >
        <span className="sr-only">Select {model.name}</span>
      </button>
      <div className="px-4 py-[0.9rem]">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1 min-w-0">
            <h3 className="text-base font-semibold text-text m-0 mb-[0.35rem] overflow-hidden text-ellipsis whitespace-nowrap flex items-center gap-2">
              {model.name}
              <IconButton
                label="Open on HuggingFace"
                size="sm"
                className="shrink-0 relative z-10"
                onClick={handleOpenHuggingFace}
              >
                <Icon icon={ExternalLink} size={14} />
              </IconButton>
            </h3>
            <span className="text-sm text-text-muted font-mono overflow-hidden text-ellipsis whitespace-nowrap">{model.id}</span>
          </div>
          <div className="flex gap-4 items-center shrink-0">
            {/* Neutral: parameter count is a fact, not a warning. */}
            {model.parameters_b && (
              <span className="px-2 py-[0.2rem] bg-background text-text-secondary rounded-sm text-xs font-mono tabular-nums">
                {model.parameters_b.toFixed(1)}B
              </span>
            )}
            {supportsTools && (
              <span 
                className="relative z-10 px-[0.35rem] py-[0.15rem] bg-primary-subtle text-primary-light rounded-sm text-sm cursor-help transition-colors duration-150 ease-linear hover:bg-primary/20"
                title="This model likely supports tool/function calling"
              >
                <Icon icon={Wrench} size={14} />
              </span>
            )}
            <span className="flex items-center gap-[0.35rem] text-sm text-text-secondary">
              <span className="text-base" aria-hidden>
                <Icon icon={Download} size={14} />
              </span>
              <span className="font-mono tabular-nums">{formatNumber(model.downloads)}</span>
            </span>
            <span className="flex items-center gap-[0.35rem] text-sm text-text-secondary">
              <span className="text-base" aria-hidden>
                <Icon icon={Heart} size={14} />
              </span>
              <span className="font-mono tabular-nums">{formatNumber(model.likes)}</span>
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
