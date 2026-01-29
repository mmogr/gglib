// TRANSPORT_EXCEPTION: Desktop-only shell integration
// This module exports platform-specific utilities that cannot be abstracted through transport.
// UI components may import from platform/, but clients/ must NOT.

// Platform detection
export { isDesktop, isWeb } from './detect';

// Shell integration
export { openUrl } from './openUrl';
export { setSelectedModel, syncMenuState, syncMenuStateSilent, setProxyState } from './menuSync';

// Desktop menu events
export { listenToMenuEvents, MENU_EVENTS } from './menuEvents';
export type { MenuEventHandlers, MenuEventType } from './menuEvents';

// File dialogs
export { pickGgufFile } from './fileDialogs';
export type { FilePickerResult } from './fileDialogs';

// Llama binary management
export { checkLlamaInstalled, installLlama, listenLlamaProgress } from './llamaInstall';
export type { LlamaStatus, LlamaInstallProgress } from './llamaInstall';

// Server logs
export { getServerLogs, listenToServerLogs } from './serverLogs';
export type { ServerLogEntry } from './serverLogs';

// Research logs (deep research session logging)
export {
  researchLogger,
  initResearchLogger,
  truncateString,
  truncatePayload,
} from './researchLogger';
export type { ResearchLogEntry, LogLevel } from './researchLogger';
