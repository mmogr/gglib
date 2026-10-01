/**
 * What a Save in the settings dialog sends: only the fields the person changed.
 *
 * The dialog reads the settings once, when it opens. Sending every field it
 * shows put each one back to what it had read, so a Save undid anything
 * written while it was open — a proxy key `gglib remote enable` stored, or a
 * port set from a terminal (#1059). Each group of fields now builds its
 * request twice, from the values on screen and from the values it loaded, the
 * same way both times, and sends the keys that differ. The backend reads an
 * absent key as "leave unchanged".
 *
 * @module components/SettingsModal/settingsRequest
 */

import type { AppSettings, SparseInferenceConfig, UpdateSettingsRequest } from '../../types';

/** Whether two request values say the same thing; objects are compared by their defined keys. */
function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].every((key) => sameValue(left[key], right[key]));
}

/** The fields of `next` whose values differ from `loaded`'s. */
export function changedFields<T extends object>(next: T, loaded: T): Partial<T> {
  const before = loaded as Record<string, unknown>;
  return Object.fromEntries(
    Object.entries(next).filter(([key, value]) => !sameValue(value, before[key])),
  ) as Partial<T>;
}

/** The General tab's own inputs, as the form holds them. */
export interface GeneralInputs {
  contextSize: string;
  proxyPort: string;
  serverPort: string;
  maxQueueSize: string;
  proxyApiKey: string;
  downloadPath: string;
  titlePrompt: string;
  maxToolIterations: string;
  showFitIndicators: boolean;
  trustClientSampling: boolean;
  defaultModel: string;
  inferenceDefaults: SparseInferenceConfig | undefined;
}

/** The inputs as saved settings fill them. */
export function generalInputs(settings: AppSettings): GeneralInputs {
  return {
    contextSize: settings.defaultContextSize?.toString() || '',
    proxyPort: settings.proxyPort?.toString() || '',
    serverPort: settings.llamaBasePort?.toString() || '',
    maxQueueSize: settings.maxDownloadQueueSize?.toString() || '',
    proxyApiKey: settings.proxyApiKey || '',
    downloadPath: settings.defaultDownloadPath || '',
    titlePrompt: settings.titleGenerationPrompt || '',
    maxToolIterations: settings.maxToolIterations?.toString() || '',
    showFitIndicators: settings.showMemoryFitIndicators !== false,
    trustClientSampling: settings.trustClientSampling === true,
    defaultModel: settings.defaultModelId?.toString() || '',
    inferenceDefaults: settings.inferenceDefaults || undefined,
  };
}

const parseNumericInput = (input: string): number | null => {
  if (!input.trim()) return null;
  const parsed = parseInt(input.trim(), 10);
  return isNaN(parsed) ? null : parsed;
};

/** Every field the inputs stand for, whether or not it changed. */
export function generalRequest(inputs: GeneralInputs): UpdateSettingsRequest {
  return {
    defaultContextSize: parseNumericInput(inputs.contextSize),
    proxyPort: parseNumericInput(inputs.proxyPort),
    llamaBasePort: parseNumericInput(inputs.serverPort),
    maxDownloadQueueSize: parseNumericInput(inputs.maxQueueSize),
    // An emptied field means "turn authentication off", which is a `null`
    // (clear the row) rather than a blank string — the backend rejects a
    // blank key precisely so it cannot mean both.
    proxyApiKey: inputs.proxyApiKey.trim() || null,
    titleGenerationPrompt: inputs.titlePrompt.trim() || null,
    maxToolIterations: parseNumericInput(inputs.maxToolIterations),
    showMemoryFitIndicators: inputs.showFitIndicators,
    defaultModelId: parseNumericInput(inputs.defaultModel),
    inferenceDefaults: inputs.inferenceDefaults,
    trustClientSampling: inputs.trustClientSampling,
    defaultDownloadPath: inputs.downloadPath.trim() || null,
  };
}
