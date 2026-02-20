import { FC } from 'react';
import { Trash2, Loader2 } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Modal } from '../../ui/Modal';
import type { GgufModel } from '../../../types';

interface DeleteModalProps {
  model: GgufModel;
  isDeleting: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

/**
 * Confirmation modal for deleting a model from the database.
 */
export const DeleteModal: FC<DeleteModalProps> = ({
  model,
  isDeleting,
  onClose,
  onConfirm,
}) => {
  const deleteLabel = isDeleting ? (
    <>
      <Loader2 className="spinner" />
      Deleting...
    </>
  ) : (
    'Delete'
  );

  return (
    <Modal
      open={true}
      onClose={onClose}
      title="Delete model"
      size="sm"
      preventClose={isDeleting}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={isDeleting}>
            Cancel
          </Button>
          <Button
            variant="danger"
            onClick={onConfirm}
            disabled={isDeleting}
            leftIcon={!isDeleting ? <Icon icon={Trash2} size={14} /> : undefined}
          >
            {deleteLabel}
          </Button>
        </>
      }
    >
      <p>Are you sure you want to remove <strong>"{model.name}"</strong> from the database?</p>
      <p className="text-text-muted text-sm mt-4">
        Note: The model file will remain on disk and won't be deleted.
      </p>
    </Modal>
  );
};
