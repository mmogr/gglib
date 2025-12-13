import { useState, useCallback } from 'react';
import type { GgufModel } from '../../../types';

export interface EditModeState {
  isEditMode: boolean;
  editedName: string;
  editedQuantization: string;
  editedFilePath: string;
  setEditedName: (name: string) => void;
  setEditedQuantization: (quant: string) => void;
  setEditedFilePath: (path: string) => void;
  handleEdit: () => void;
  handleCancel: () => void;
  resetEditState: () => void;
}

/**
 * Hook for managing edit mode state in the ModelInspectorPanel.
 * Handles toggling edit mode and tracking edited field values.
 */
export function useEditMode(model: GgufModel | null): EditModeState {
  const [isEditMode, setIsEditMode] = useState(false);
  const [editedName, setEditedName] = useState('');
  const [editedQuantization, setEditedQuantization] = useState('');
  const [editedFilePath, setEditedFilePath] = useState('');

  const handleEdit = useCallback(() => {
    if (!model) return;
    setEditedName(model.name);
    setEditedQuantization(model.quantization || '');
    setEditedFilePath(model.file_path);
    setIsEditMode(true);
  }, [model]);

  const handleCancel = useCallback(() => {
    setIsEditMode(false);
  }, []);

  const resetEditState = useCallback(() => {
    setIsEditMode(false);
    setEditedName('');
    setEditedQuantization('');
    setEditedFilePath('');
  }, []);

  return {
    isEditMode,
    editedName,
    editedQuantization,
    editedFilePath,
    setEditedName,
    setEditedQuantization,
    setEditedFilePath,
    handleEdit,
    handleCancel,
    resetEditState,
  };
}
