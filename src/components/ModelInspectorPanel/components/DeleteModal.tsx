import { FC } from 'react';
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
  return (
    <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && !isDeleting && onClose()}>
      <div className="modal modal-sm">
        <div className="modal-header">
          <h3>Delete Model</h3>
          <button
            className="modal-close"
            onClick={onClose}
            disabled={isDeleting}
          >
            ✕
          </button>
        </div>

        <div className="modal-body">
          <p>Are you sure you want to remove <strong>"{model.name}"</strong> from the database?</p>
          <p className="text-muted" style={{ marginTop: 'var(--spacing-base)' }}>
            Note: The model file will remain on disk and won't be deleted.
          </p>
        </div>

        <div className="modal-footer">
          <button
            className="btn btn-secondary"
            onClick={onClose}
            disabled={isDeleting}
          >
            Cancel
          </button>
          <button
            className="btn btn-danger"
            onClick={onConfirm}
            disabled={isDeleting}
          >
            {isDeleting ? (
              <>
                <span className="spinner"></span>
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
