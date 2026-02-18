import { FC } from 'react';
import { AlertTriangle, Loader2, Trash2 } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Modal } from '../ui/Modal';

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
      <Loader2 className="w-4 h-4 animate-spin" />
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
      <div className="flex flex-col gap-spacing-md">
        <div className="flex items-start gap-spacing-sm">
          <span className="w-9 h-9 rounded-full inline-flex items-center justify-center bg-background-secondary border border-border text-primary">
            <Icon icon={Trash2} size={18} />
          </span>
          <div>
            <h2 className="m-0 text-base font-semibold text-text">Delete this message?</h2>
            <p className="mt-[0.15rem] mb-0 text-text-secondary text-[0.95rem]">This action is permanent.</p>
          </div>
        </div>

        {messageCount > 1 && (
          <div className="flex gap-spacing-sm items-start p-3 rounded-lg bg-[rgba(239,68,68,0.08)] border border-[rgba(239,68,68,0.25)] text-text text-[0.95rem]">
            <Icon icon={AlertTriangle} size={16} className="text-danger" />
            <span>
              This will also delete <strong>{messageCount - 1}</strong> subsequent{' '}
              {messageCount - 1 === 1 ? 'message' : 'messages'} to maintain conversation flow.
            </span>
          </div>
        )}

        <div className="flex justify-end gap-spacing-sm">
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
