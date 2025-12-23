import { FC } from 'react';
import { AlertTriangle, Loader2, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Modal } from '../ui/Modal';
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

  const deleteLabel = isDeleting ? (
    <>
      <Loader2 className={styles.spinnerIcon} />
      Deleting...
    </>
  ) : (
    'Delete'
  );

  return (
    <Modal
      open={isOpen}
      onClose={onCancel}
      title="Delete message"
      size="sm"
      preventClose={isDeleting}
    >
      <div className={styles.content}>
        <div className={styles.lede}>
          <span className={styles.iconCircle}>
            <Icon icon={Trash2} size={18} />
          </span>
          <div>
            <h2 className={styles.title}>Delete this message?</h2>
            <p className={styles.description}>This action is permanent.</p>
          </div>
        </div>

        {messageCount > 1 && (
          <div className={styles.warning}>
            <Icon icon={AlertTriangle} size={16} className={styles.warningIcon} />
            <span>
              This will also delete <strong>{messageCount - 1}</strong> subsequent{' '}
              {messageCount - 1 === 1 ? 'message' : 'messages'} to maintain conversation flow.
            </span>
          </div>
        )}

        <div className={styles.actions}>
          <Button variant="ghost" onClick={onCancel} disabled={isDeleting}>
            Cancel
          </Button>
          <Button variant="danger" onClick={onConfirm} disabled={isDeleting} leftIcon={isDeleting ? undefined : <Icon icon={Trash2} size={14} />}>
            {deleteLabel}
          </Button>
        </div>
      </div>
    </Modal>
  );
};
