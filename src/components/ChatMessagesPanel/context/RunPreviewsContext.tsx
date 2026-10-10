import { createContext, useContext } from 'react';
import { NO_PREVIEWS, type RunPreviews } from '../../../hooks/useGglibRuntime/runPreviews';

/**
 * The preview frames of the run being read, by tool call, for the reply
 * that shows them. They reach a reply this way and never through its
 * message, so nothing that keeps a message keeps a frame.
 */
const RunPreviewsContext = createContext<RunPreviews>(NO_PREVIEWS);

export const RunPreviewsProvider = RunPreviewsContext.Provider;

/** The frames held; none outside a provider. */
export function useRunPreviews(): RunPreviews {
  return useContext(RunPreviewsContext);
}
