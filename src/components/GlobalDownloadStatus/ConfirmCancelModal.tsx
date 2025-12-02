import { FC } from 'react';
import styles from './ConfirmCancelModal.module.css';

interface ConfirmCancelModalProps {
  isOpen: boolean;
  modelId: string;
  /** Whether this is a sharded download */
  isSharded: boolean;
  /** Number of shards (only relevant for sharded downloads) */
  shardCount?: number;
  /** Current shard being downloaded (1-indexed for display) */
  currentShard?: number;
  isCancelling: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Modal dialog for confirming download cancellation.
 * Shows additional context for sharded downloads.
 */
export const ConfirmCancelModal: FC<ConfirmCancelModalProps> = ({
  isOpen,
  modelId,
  isSharded,
  shardCount,
  currentShard,
  isCancelling,
  onConfirm,
  onCancel,
}) => {
  if (!isOpen) return null;

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget && !isCancelling) {
      onCancel();
    }
  };

  // Extract display name from modelId (e.g., "unsloth/Model:Q4_K_M" -> "Model (Q4_K_M)")
  const [repoId, quant] = modelId.split(':');
  const modelName = repoId.split('/').pop() || repoId;
  const displayName = quant ? `${modelName} (${quant})` : modelName;

  return (
    <div className={styles.overlay} onClick={handleOverlayClick}>
      <div className={styles.modal}>
        <div className={styles.icon}>⏹️</div>
        
        <h2 className={styles.title}>Cancel Download?</h2>
        
        <p className={styles.description}>
          Stop downloading <strong>{displayName}</strong>?
        </p>

        {isSharded && shardCount && shardCount > 1 && (
          <div className={styles.shardInfo}>
            <span className={styles.shardIcon}>📦</span>
            {currentShard !== undefined ? (
              <>
                Currently downloading shard {currentShard} of {shardCount}.
                <br />
                All remaining shards will be cancelled.
              </>
            ) : (
              <>This is a sharded download with {shardCount} parts.</>
            )}
          </div>
        )}

        <div className={styles.actions}>
          <button 
            className={styles.keepButton}
            onClick={onCancel}
            disabled={isCancelling}
          >
            Keep Downloading
          </button>
          <button 
            className={styles.cancelButton}
            onClick={onConfirm}
            disabled={isCancelling}
          >
            {isCancelling ? (
              <>
                <span className={styles.spinner} />
                Cancelling...
              </>
            ) : (
              'Cancel Download'
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
