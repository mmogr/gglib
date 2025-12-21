import { FC } from 'react';
import styles from './ConfirmDeleteModal.module.css';

interface ConfirmDeleteModalProps {
  isOpen: boolean;
  messageCount: number;
  isDeleting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Modal dialog for confirming message deletion.
 * Warns the user about cascade deletion of subsequent messages.
 */
export const ConfirmDeleteModal: FC<ConfirmDeleteModalProps> = ({
  isOpen,
  messageCount,
  isDeleting,
  onConfirm,
  onCancel,
}) => {
  if (!isOpen) return null;

  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget && !isDeleting) {
      onCancel();
    }
  };

  return (
    <div className={styles.overlay} onClick={handleOverlayClick}>
      <div className={styles.modal}>
        <div className={styles.icon}>🗑️</div>
        
        <h2 className={styles.title}>Delete Message?</h2>
        
        <p className={styles.description}>
          This will permanently delete this message.
        </p>

        {messageCount > 1 && (
          <div className={styles.warning}>
            <span className={styles.warningIcon}>⚠️</span>
            This will also delete <strong>{messageCount - 1}</strong> subsequent{' '}
            {messageCount - 1 === 1 ? 'message' : 'messages'} to maintain conversation flow.
          </div>
        )}

        <div className={styles.actions}>
          <button 
            className={styles.cancelButton}
            onClick={onCancel}
            disabled={isDeleting}
          >
            Cancel
          </button>
          <button 
            className={styles.deleteButton}
            onClick={onConfirm}
            disabled={isDeleting}
          >
            {isDeleting ? (
              <>
                <span className={styles.spinner} />
                Deleting...
              </>
            ) : (
              'Delete'
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
