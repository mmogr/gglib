import { FC } from "react";
import { Download, ExternalLink, Heart, Wrench } from "lucide-react";
import { HfModelSummary } from "../../../types";
import { formatNumber, getHuggingFaceModelUrl } from "../../../utils/format";
import { useToolSupportCache } from "../../../hooks/useToolSupportCache";
import { openUrl } from "../../../services/platform";
import { Icon } from "../../ui/Icon";
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
        'bg-[rgba(255,255,255,0.03)] border border-[rgba(255,255,255,0.08)] rounded-xl mb-3 overflow-hidden transition-all duration-200 ease-linear hover:bg-[rgba(255,255,255,0.05)] hover:border-[rgba(255,255,255,0.12)]',
        isSelected && 'border-[rgba(34,211,238,0.5)] bg-[rgba(34,211,238,0.08)] hover:border-[rgba(34,211,238,0.6)] hover:bg-[rgba(34,211,238,0.1)]'
      )}
      onClick={onSelect}
    >
      <div className="px-4 py-[0.9rem] cursor-pointer">
        <div className="flex items-start justify-between gap-4">
          <div className="flex-1 min-w-0">
            <h3 className="text-base font-semibold text-[#f1f5f9] m-0 mb-[0.35rem] overflow-hidden text-ellipsis whitespace-nowrap flex items-center gap-2">
              {model.name}
              <button
                className="bg-none border-none cursor-pointer text-base px-[0.3rem] py-[0.15rem] rounded-sm opacity-70 transition-all duration-200 ease-linear shrink-0 hover:opacity-100 hover:bg-[rgba(255,255,255,0.1)] hover:scale-110 active:scale-95"
                onClick={handleOpenHuggingFace}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
              >
                <Icon icon={ExternalLink} size={14} />
              </button>
            </h3>
            <span className="text-[0.8rem] text-[#64748b] font-mono overflow-hidden text-ellipsis whitespace-nowrap">{model.id}</span>
          </div>
          <div className="flex gap-4 items-center shrink-0">
            {model.parameters_b && (
              <span className="px-2 py-[0.2rem] bg-[rgba(251,191,36,0.15)] text-[#fbbf24] rounded-sm text-[0.75rem] font-semibold">
                {model.parameters_b.toFixed(1)}B
              </span>
            )}
            {supportsTools && (
              <span 
                className="px-[0.35rem] py-[0.15rem] bg-[rgba(96,165,250,0.15)] rounded-sm text-[0.8rem] cursor-help transition-colors duration-150 ease-linear hover:bg-[rgba(96,165,250,0.25)]"
                title="This model likely supports tool/function calling"
              >
                <Icon icon={Wrench} size={14} />
              </span>
            )}
            <span className="flex items-center gap-[0.35rem] text-[0.8rem] text-[#94a3b8]">
              <span className="text-[0.9rem]" aria-hidden>
                <Icon icon={Download} size={14} />
              </span>
              {formatNumber(model.downloads)}
            </span>
            <span className="flex items-center gap-[0.35rem] text-[0.8rem] text-[#94a3b8]">
              <span className="text-[0.9rem]" aria-hidden>
                <Icon icon={Heart} size={14} />
              </span>
              {formatNumber(model.likes)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
