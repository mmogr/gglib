/**
 * Composed Transport interface.
 * Combines all domain sub-interfaces into a single unified transport.
 */

// Re-export ID types
export * from './ids';

// Re-export common types
export * from './common';

// Re-export sub-interface types
export * from './models';
export * from './tags';
export * from './settings';
export * from './servers';
export * from './proxy';
export * from './downloads';
export * from './mcp';
export * from './events';
export * from './chat';

// Import sub-interfaces for composition
import type { ModelsTransport } from './models';
import type { TagsTransport } from './tags';
import type { SettingsTransport } from './settings';
import type { ServersTransport } from './servers';
import type { ProxyTransport } from './proxy';
import type { DownloadsTransport } from './downloads';
import type { McpTransport } from './mcp';
import type { EventsTransport } from './events';
import type { ChatTransport } from './chat';

/**
 * Unified transport interface.
 * 
 * This is the primary abstraction between frontend domain logic and
 * platform-specific communication (Tauri IPC vs HTTP).
 * 
 * Transport selection happens once at composition root via `getTransport()`.
 * Domain clients and hooks should never import transport implementations directly.
 */
export interface Transport
  extends ModelsTransport,
    TagsTransport,
    SettingsTransport,
    ServersTransport,
    ProxyTransport,
    DownloadsTransport,
    McpTransport,
    EventsTransport,
    ChatTransport {}
