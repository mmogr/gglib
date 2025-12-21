/**
 * Event name constants matching backend AppEvent.event_name() outputs.
 * 
 * These constants ensure the frontend subscribes to the exact event names
 * emitted by the Rust backend, maintaining single source of truth in the domain.
 * 
 * @see crates/gglib-core/src/events/mod.rs - AppEvent::event_name()
 */

/**
 * Download-related event names.
 */
export const DOWNLOAD_EVENT_NAMES = [
  'download:started',
  'download:progress',
  'download:completed',
  'download:failed',
  'download:cancelled',
  'download:queue_snapshot',
  'download:queue_run_complete',
] as const;

/**
 * Server-related event names.
 */
export const SERVER_EVENT_NAMES = [
  'server:started',
  'server:stopped',
  'server:error',
  'server:snapshot',
] as const;

/**
 * Log-related event names.
 */
export const LOG_EVENT_NAMES = [
  'log:entry',
] as const;

/**
 * MCP server-related event names.
 */
export const MCP_EVENT_NAMES = [
  'mcp:added',
  'mcp:removed',
  'mcp:started',
  'mcp:stopped',
  'mcp:error',
] as const;

/**
 * Model-related event names.
 */
export const MODEL_EVENT_NAMES = [
  'model:added',
  'model:removed',
  'model:updated',
] as const;

/**
 * Type helper to extract event name literals.
 */
export type DownloadEventName = typeof DOWNLOAD_EVENT_NAMES[number];
export type ServerEventName = typeof SERVER_EVENT_NAMES[number];
export type LogEventName = typeof LOG_EVENT_NAMES[number];
export type McpEventName = typeof MCP_EVENT_NAMES[number];
export type ModelEventName = typeof MODEL_EVENT_NAMES[number];
