/**
 * Shared logging types and interfaces.
 * 
 * This module defines the core abstractions for the unified logging system.
 * Both AppLogger and researchLogger implement these interfaces.
 */

// =============================================================================
// Log Levels
// =============================================================================

/**
 * Standard log levels matching Rust tracing.
 * Ordered by severity: debug < info < warn < error
 */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

/**
 * Numeric values for log level comparison.
 */
const LOG_LEVEL_VALUES: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

/**
 * Check if a log level should be emitted given the minimum level.
 * 
 * @param level - The level of the log entry
 * @param minLevel - The minimum level to emit
 * @returns true if the log should be emitted
 * 
 * @example
 * isLevelEnabled('debug', 'info') // false
 * isLevelEnabled('warn', 'info')  // true
 * isLevelEnabled('error', 'error') // true
 */
export function isLevelEnabled(level: LogLevel, minLevel: LogLevel): boolean {
  return LOG_LEVEL_VALUES[level] >= LOG_LEVEL_VALUES[minLevel];
}

/**
 * Parse a log level string, returning a default if invalid.
 * 
 * @param value - The log level string to parse
 * @param defaultLevel - Default level if parsing fails
 * @returns A valid LogLevel
 */
export function parseLogLevel(value: string | undefined, defaultLevel: LogLevel = 'info'): LogLevel {
  if (!value) return defaultLevel;
  
  const normalized = value.toLowerCase();
  if (normalized === 'debug' || normalized === 'info' || normalized === 'warn' || normalized === 'error') {
    return normalized;
  }
  
  return defaultLevel;
}

// =============================================================================
// Log Entry
// =============================================================================

/**
 * Structured log entry base interface.
 * Represents a single log event with metadata.
 */
export interface LogEntry {
  /** ISO timestamp (e.g., '2026-02-01T10:30:45.123Z') */
  timestamp: string;
  
  /** Log level */
  level: LogLevel;
  
  /** Category for filtering (e.g., 'transport.api', 'research.loop') */
  category: string;
  
  /** Human-readable message */
  message: string;
  
  /** Optional structured data (will be truncated if too large) */
  data?: Record<string, unknown>;
}

// =============================================================================
// Logger Interface
// =============================================================================

/**
 * Core logger interface - transport-agnostic.
 * 
 * All logger implementations (AppLogger, researchLogger wrapper) should
 * implement this interface for consistency.
 */
export interface ILogger {
  /**
   * Log a message with optional structured data.
   * 
   * @param level - Log level
   * @param category - Category for filtering
   * @param message - Human-readable message
   * @param data - Optional structured metadata
   */
  log(level: LogLevel, category: string, message: string, data?: Record<string, unknown>): void;
  
  /**
   * Log a debug message.
   * Lowest severity - typically only shown in development.
   */
  debug(category: string, message: string, data?: Record<string, unknown>): void;
  
  /**
   * Log an informational message.
   * Normal operational messages.
   */
  info(category: string, message: string, data?: Record<string, unknown>): void;
  
  /**
   * Log a warning message.
   * Potentially harmful situations.
   */
  warn(category: string, message: string, data?: Record<string, unknown>): void;
  
  /**
   * Log an error message.
   * Error events that might still allow the app to continue.
   */
  error(category: string, message: string, data?: Record<string, unknown>): void;
}
