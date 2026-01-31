/**
 * Application Logger
 * 
 * Unified logging system for the entire application with:
 * - Strictly typed categories (no magic strings)
 * - Frontend-side level filtering (performance)
 * - Non-blocking IPC transport to Rust backend
 * - Configurable via VITE_LOG_LEVEL environment variable
 */

import type { ILogger, LogEntry, LogLevel } from './types';
import { isLevelEnabled, parseLogLevel } from './types';
import type { ILogTransport } from './transports';
import { ConsoleTransport, TauriTracingTransport } from './transports';
import { truncatePayload } from '../researchLogger';
import { isDesktop } from '../detect';

// =============================================================================
// Strictly Typed Categories
// =============================================================================

/**
 * Application log categories - strictly typed union.
 * 
 * No magic strings allowed - all categories must be declared here.
 * Organized by domain for easy discovery.
 */
export type AppLogCategory =
  // Transport layer
  | 'transport'
  | 'transport.api'
  | 'transport.sse'
  | 'transport.tauri'
  | 'transport.events'
  
  // Services
  | 'service.download'
  | 'service.server'
  | 'service.mcp'
  | 'service.chat'
  | 'service.settings'
  | 'service.platform'
  
  // Research system
  | 'research.session'
  | 'research.loop'
  | 'research.tool'
  | 'research.fact'
  | 'research.llm'
  | 'research.planning'
  
  // Data flow
  | 'decoder'
  | 'validation'
  | 'persistence'
  | 'serialization'
  
  // UI layers
  | 'component'
  | 'hook'
  | 'context'
  | 'page'
  
  // Platform integration
  | 'platform.menu'
  | 'platform.file'
  | 'platform.system'
  | 'platform.ipc'
  
  // Development & debugging
  | 'dev.debug'
  | 'dev.performance'
  | 'dev.hotreload';

// =============================================================================
// AppLogger Implementation
// =============================================================================

/**
 * Application logger with configurable transports.
 * 
 * Unlike researchLogger (session-based), this is a global singleton for
 * general application logging throughout the codebase.
 * 
 * Key features:
 * - Frontend-side level filtering (checks VITE_LOG_LEVEL before transport iteration)
 * - Synchronous transport iteration (transports handle async internally)
 * - Payload truncation to prevent memory/IPC bloat
 * - Never throws - logging failures are silent
 */
class AppLogger implements ILogger {
  private transports: ILogTransport[] = [];
  private minLevel: LogLevel = 'info';
  
  /**
   * Set the minimum log level.
   * Logs below this level are filtered out before reaching transports.
   * 
   * @param level - Minimum level to emit
   */
  setMinLevel(level: LogLevel): void {
    this.minLevel = level;
  }
  
  /**
   * Get the current minimum log level.
   */
  getMinLevel(): LogLevel {
    return this.minLevel;
  }
  
  /**
   * Add a transport to the logger.
   * Transports define where logs go (console, files, Tauri, etc.)
   * 
   * @param transport - Transport implementation
   */
  addTransport(transport: ILogTransport): void {
    this.transports.push(transport);
  }
  
  /**
   * Remove all transports.
   * Useful for testing or reconfiguration.
   */
  clearTransports(): void {
    this.transports = [];
  }
  
  /**
   * Log a message with optional structured data.
   * 
   * PERFORMANCE: This method checks the log level FIRST before doing any
   * work. If the level is filtered out, returns immediately (no transport
   * iteration, no payload processing).
   * 
   * @param level - Log level
   * @param category - Strictly typed category
   * @param message - Human-readable message
   * @param data - Optional structured metadata (will be truncated)
   */
  log(level: LogLevel, category: AppLogCategory, message: string, data?: Record<string, unknown>): void {
    // CRITICAL: Frontend-side filtering for performance
    // Short-circuit if this log level is filtered out
    if (!isLevelEnabled(level, this.minLevel)) {
      return;
    }
    
    // Create log entry with truncated payload
    const entry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      category,
      message,
      data: data ? (truncatePayload(data) as Record<string, unknown>) : undefined,
    };
    
    // Write to all transports synchronously
    // Transports handle async operations internally (fire-and-forget)
    for (const transport of this.transports) {
      try {
        transport.write(entry);
      } catch (error) {
        // Never throw from logging - silently ignore transport failures
        // In dev mode, the transport itself might log to console
      }
    }
  }
  
  /**
   * Log a debug message.
   * Lowest severity - typically only shown in development.
   */
  debug(category: AppLogCategory, message: string, data?: Record<string, unknown>): void {
    this.log('debug', category, message, data);
  }
  
  /**
   * Log an informational message.
   * Normal operational messages.
   */
  info(category: AppLogCategory, message: string, data?: Record<string, unknown>): void {
    this.log('info', category, message, data);
  }
  
  /**
   * Log a warning message.
   * Potentially harmful situations that should be investigated.
   */
  warn(category: AppLogCategory, message: string, data?: Record<string, unknown>): void {
    this.log('warn', category, message, data);
  }
  
  /**
   * Log an error message.
   * Error events that might still allow the app to continue.
   */
  error(category: AppLogCategory, message: string, data?: Record<string, unknown>): void {
    this.log('error', category, message, data);
  }
}

// =============================================================================
// Singleton Export
// =============================================================================

/**
 * Global application logger instance.
 * 
 * Import this singleton and use its methods directly:
 * 
 * @example
 * ```ts
 * import { appLogger } from 'services/platform/logging';
 * 
 * appLogger.info('transport.api', 'API request completed', { 
 *   method: 'GET',
 *   url: '/models',
 *   duration: 123
 * });
 * ```
 */
export const appLogger = new AppLogger();

// =============================================================================
// Initialization
// =============================================================================

/**
 * Initialize the application logger with default transports.
 * 
 * Call once at app startup (typically in main.tsx).
 * 
 * Configuration:
 * - Console transport: Always enabled in dev mode
 * - Tauri transport: Enabled on desktop (sends logs to Rust backend)
 * - Min level: Controlled by VITE_LOG_LEVEL env var (default: 'info' in prod, 'debug' in dev)
 * 
 * @example
 * ```ts
 * // In main.tsx
 * await initAppLogger();
 * ```
 */
export async function initAppLogger(): Promise<void> {
  // Read log level from environment
  const envLevel = import.meta.env.VITE_LOG_LEVEL as string | undefined;
  const defaultLevel: LogLevel = import.meta.env.DEV ? 'debug' : 'info';
  const minLevel = parseLogLevel(envLevel, defaultLevel);
  
  appLogger.setMinLevel(minLevel);
  
  // Always add console transport in development
  if (import.meta.env.DEV) {
    appLogger.addTransport(new ConsoleTransport(true));
  }
  
  // Add Tauri transport on desktop for persistence
  if (isDesktop()) {
    appLogger.addTransport(new TauriTracingTransport(true));
  }
  
  // Log initialization
  appLogger.info('platform.system', 'Application logger initialized', {
    minLevel,
    transports: appLogger['transports'].length,
    platform: isDesktop() ? 'desktop' : 'web',
  });
}
