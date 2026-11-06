/**
 * State for the drawing settings: the default image model and whether
 * MCP clients may draw through `/mcp`.
 *
 * Same rationale as `useDesktopSettings`: the group owns its own state and
 * hands back a ready-made slice of the update request, holding only what
 * changed since the settings loaded.
 *
 * @module components/SettingsModal/useDrawingSettings
 */

import { useCallback, useEffect, useState } from 'react';
import type { AppSettings, UpdateSettingsRequest } from '../../types';
import { drawingValuesFrom, type DrawingSettingsValues } from '../../readings/drawingValues';
import { changedFields } from './settingsRequest';

const DEFAULTS: DrawingSettingsValues = {
  defaultImageModel: '',
  mcpDrawing: false,
};

export interface UseDrawingSettingsResult {
  values: DrawingSettingsValues;
  setValue: <K extends keyof DrawingSettingsValues>(
    key: K,
    value: DrawingSettingsValues[K],
  ) => void;
  /** The fields changed since settings loaded, as update-request fields. */
  updates: Partial<Pick<UpdateSettingsRequest, 'defaultImageModelId' | 'mcpDrawing'>>;
}

const requestFrom = (values: DrawingSettingsValues) => {
  const id = parseInt(values.defaultImageModel, 10);
  return {
    // Blank clears the setting, which is `null` on the wire.
    defaultImageModelId: Number.isFinite(id) ? id : null,
    mcpDrawing: values.mcpDrawing,
  };
};

/** Track the drawing fields, seeded from persisted settings. */
export function useDrawingSettings(settings: AppSettings | null): UseDrawingSettingsResult {
  const [values, setValues] = useState<DrawingSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues(drawingValuesFrom(settings));
    }
  }, [settings]);

  const setValue = useCallback(
    <K extends keyof DrawingSettingsValues>(key: K, value: DrawingSettingsValues[K]) => {
      setValues((previous) => ({ ...previous, [key]: value }));
    },
    [],
  );

  const loaded = settings ? drawingValuesFrom(settings) : DEFAULTS;
  return { values, setValue, updates: changedFields(requestFrom(values), requestFrom(loaded)) };
}
