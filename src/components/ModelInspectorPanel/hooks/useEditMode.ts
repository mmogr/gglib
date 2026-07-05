import { useState, useCallback } from 'react';
import type { GgufModel, InferenceConfig, ServerConfig } from '../../../types';

export interface EditModeState {
  isEditMode: boolean;
  editedName: string;
  editedQuantization: string;
  editedFilePath: string;
  editedInferenceDefaults: InferenceConfig | undefined;
  editedServerDefaults: ServerConfig | null | undefined;
  setEditedName: (name: string) => void;
  setEditedQuantization: (quant: string) => void;
  setEditedFilePath: (path: string) => void;
  setEditedInferenceDefaults: (config: InferenceConfig) => void;
  setEditedServerDefaults: (config: ServerConfig | null) => void;
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
  const [editedInferenceDefaults, setEditedInferenceDefaults] = useState<InferenceConfig | undefined>(undefined);
  const [editedServerDefaults, setEditedServerDefaults] = useState<ServerConfig | null | undefined>(undefined);

  const handleEdit = useCallback(() => {
    if (!model) return;
    setEditedName(model.name);
    setEditedQuantization(model.quantization || '');
    setEditedFilePath(model.filePath);
    setEditedInferenceDefaults(model.inferenceDefaults || undefined);
    setEditedServerDefaults(model.serverDefaults ?? undefined);
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
    setEditedInferenceDefaults(undefined);
    setEditedServerDefaults(undefined);
  }, []);

  return {
    isEditMode,
    editedName,
    editedQuantization,
    editedFilePath,
    editedInferenceDefaults,
    editedServerDefaults,
    setEditedName,
    setEditedQuantization,
    setEditedFilePath,
    setEditedInferenceDefaults,
    setEditedServerDefaults,
    handleEdit,
    handleCancel,
    resetEditState,
  };
}
