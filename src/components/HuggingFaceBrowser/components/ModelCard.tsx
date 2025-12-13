import { FC } from "react";
import { HfModelSummary } from "../../../types";
import { formatNumber, getHuggingFaceModelUrl } from "../../../utils/format";
import { useToolSupportCache } from "../../../hooks/useToolSupportCache";
import { openUrl } from "../../../services/platform";
import styles from "../HuggingFaceBrowser.module.css";

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
      className={`${styles.modelCard} ${isSelected ? styles.modelCardSelected : ''}`}
      onClick={onSelect}
    >
      <div className={styles.modelCardHeader}>
        <div className={styles.modelCardMain}>
          <div className={styles.modelInfo}>
            <h3 className={styles.modelName}>
              {model.name}
              <button
                className={styles.hfButton}
                onClick={handleOpenHuggingFace}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
              >
                🤗
              </button>
            </h3>
            <span className={styles.modelId}>{model.id}</span>
          </div>
          <div className={styles.modelStats}>
            {model.parameters_b && (
              <span className={styles.paramBadge}>
                {model.parameters_b.toFixed(1)}B
              </span>
            )}
            {supportsTools && (
              <span 
                className={styles.toolIcon}
                title="This model likely supports tool/function calling"
              >
                🔧
              </span>
            )}
            <span className={styles.stat}>
              <span className={styles.statIcon}>⬇️</span>
              {formatNumber(model.downloads)}
            </span>
            <span className={styles.stat}>
              <span className={styles.statIcon}>❤️</span>
              {formatNumber(model.likes)}
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
