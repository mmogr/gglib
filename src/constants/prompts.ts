/**
 * Application-wide prompt string constants.
 *
 * These are fixed values (not environment configuration) shared across UI
 * components and the runtime layer.  Keeping them here prevents the runtime
 * hook module (`useGglibRuntime/streamAgentChat.ts`) from carrying UI-facing
 * defaults, and gives consumers a single canonical source of truth.
 */

/** Default system prompt shown in the UI and used as the conversation default. */
export const DEFAULT_SYSTEM_PROMPT = 'You are a helpful assistant.';
