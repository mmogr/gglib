/**
 * The drawing settings as the settings page reads them: the default image
 * model and whether MCP clients may draw, from the stored `AppSettings`. The
 * contract test `settingsBounds.test.ts` holds the reading to the stored
 * defaults.
 *
 * @module readings/drawingValues
 */

import type { AppSettings } from '../types';

export interface DrawingSettingsValues {
  /** The chosen image model's id as the select holds it; blank = none. */
  defaultImageModel: string;
  /** Unset on the wire reads as off. */
  mcpDrawing: boolean;
}

export const drawingValuesFrom = (settings: AppSettings): DrawingSettingsValues => ({
  defaultImageModel: settings.defaultImageModelId?.toString() ?? '',
  // Off unless switched on: the backend reads an absent value the same way.
  mcpDrawing: settings.mcpDrawing === true,
});
