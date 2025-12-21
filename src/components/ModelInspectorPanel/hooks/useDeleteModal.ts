import { useState, useCallback } from 'react';

export interface DeleteModalState {
  showDeleteModal: boolean;
  isDeleting: boolean;
  setShowDeleteModal: (show: boolean) => void;
  setIsDeleting: (deleting: boolean) => void;
  openDeleteModal: () => void;
  closeDeleteModal: () => void;
}

/**
 * Hook for managing delete confirmation modal state.
 */
export function useDeleteModal(): DeleteModalState {
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  const openDeleteModal = useCallback(() => {
    setShowDeleteModal(true);
  }, []);

  const closeDeleteModal = useCallback(() => {
    if (!isDeleting) {
      setShowDeleteModal(false);
    }
  }, [isDeleting]);

  return {
    showDeleteModal,
    isDeleting,
    setShowDeleteModal,
    setIsDeleting,
    openDeleteModal,
    closeDeleteModal,
  };
}
