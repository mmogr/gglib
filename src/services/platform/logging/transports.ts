/**
 * Log transport implementations.
 * 
 * Transports define where logs go: console, files, Tauri IPC, etc.
 * All transports implement the ILogTransport interface.
 */

import type { LogEntry } from './types';
import { truncatePayload } from '../researchLogger';

// =============================================================================
// Transport Interface
// =============================================================================

/**
 * Log transport interface - where logs go.
 * 
 * Implementations must handle all errors internally and never throw.
 * Write operations should be fast and non-blocking.
 */
export interface ILogTransport {
  /**
   * Write a log entry to the transport.
   * 
   * Must not throw - implementations must handle errors internally.
   * Should be non-blocking for performance.
   * 
   * @param entry - The log entry to write
   */
  write(entry: LogEntry): void | Promise<void>;
  
  /**
   * Flush any buffered logs (optional).
   * 
   * Called during graceful shutdown or when immediate persistence is needed.
   */
  flush?(): void | Promise<void>;
}

// =============================================================================
// Console Transport
// =============================================================================

/**
 * Console transport - writes to browser console.
 * 
 * Typically used in development mode only. In production, logs should
 * go to files/telemetry instead of console.
 */
export class ConsoleTransport implements ILogTransport {
  /**
   * @param devOnly - Only write to console in dev mode (default: true)
   */
  constructor(private readonly devOnly: boolean = true) {}
  
  write(entry: LogEntry): void {
    // Skip in production if devOnly is enabled
    if (this.devOnly && !import.meta.env.DEV) {
      return;
    }
    
    const prefix = `[${entry.category}]`;
    const args: unknown[] = [prefix, entry.message];
    
    // Append data if present
    if (entry.data && Object.keys(entry.data).length > 0) {
      args.push(entry.data);
    }
    
    // Route to appropriate console method
    switch (entry.level) {
      case 'debug':
        console.debug(...args);
        break;
      case 'info':
        console.log(...args);
        break;
      case 'warn':
        console.warn(...args);
        break;
      case 'error':
        console.error(...args);
        break;
    }
  }
}

// =============================================================================
// Tauri Tracing Transport
// =============================================================================

/**
 * Tauri tracing transport - sends logs to Rust backend via IPC.
 * 
 * This transport bridges frontend logs into the Rust tracing infrastructure.
 * Logs are written to files via tracing-appender and appear in stdout.
 * 
 * CRITICAL: Uses fire-and-forget IPC - never awaits promises to avoid
 * blocking the UI thread. Logs may be lost if IPC fails, but this is
 * acceptable for performance.
 */
export class TauriTracingTransport implements ILogTransport {
  /**
   * @param enabled - Whether this transport is enabled (default: true)
   */
  constructor(private readonly enabled: boolean = true) {}
  
  write(entry: LogEntry): void {
    if (!this.enabled) return;
    
    // Check if Tauri IPC is available
    if (!this.hasTauriInvoke()) return;
    
    // Truncate data payload to prevent IPC message bloat
    const truncatedEntry: LogEntry = {
      ...entry,
      data: entry.data ? (truncatePayload(entry.data) as Record<string, unknown>) : undefined,
    };
    
    // Fire-and-forget IPC call - do NOT await
    // This ensures the UI thread never blocks on logging
    this.invokeAsync(truncatedEntry).catch(() => {
      // Silently ignore IPC errors - logging failures should never crash the app
      // In development, errors will appear in console from the invoke call itself
    });
  }
  
  /**
   * Check if Tauri invoke API is available.
   */
  private hasTauriInvoke(): boolean {
    return !!(
      typeof window !== 'undefined' &&
      window.__TAURI_INTERNALS__ &&
      typeof window.__TAURI_INTERNALS__.invoke === 'function'
    );
  }
  
  /**
   * Invoke Tauri command asynchronously (fire-and-forget).
   * 
   * This method is async but callers should NOT await it to maintain
   * non-blocking behavior.
   */
  private async invokeAsync(entry: LogEntry): Promise<void> {
    if (!window.__TAURI_INTERNALS__) return;
    
    // Convert LogEntry to format expected by Rust backend
    await window.__TAURI_INTERNALS__.invoke('log_from_frontend', {
      entry: {
        timestamp: entry.timestamp,
        level: entry.level,
        category: entry.category,
        message: entry.message,
        data: entry.data ? JSON.stringify(entry.data) : null,
      },
    });
  }
}

// =============================================================================
// Multi Transport
// =============================================================================

/**
 * Multi-transport logger - writes to multiple transports.
 * 
 * Useful for combining console logging (dev) with file logging (production).
 * Failures in one transport don't affect others.
 */
export class MultiTransport implements ILogTransport {
  /**
   * @param transports - Array of transports to write to
   */
  constructor(private readonly transports: ILogTransport[]) {}
  
  write(entry: LogEntry): void {
    for (const transport of this.transports) {
      try {
        // Call write without awaiting - each transport handles async internally
        transport.write(entry);
      } catch (error) {
        // Don't let one transport failure break others
        // In dev mode, this might appear in console
        if (import.meta.env.DEV) {
          console.error('[MultiTransport] Transport write failed:', error);
        }
      }
    }
  }
  
  async flush(): Promise<void> {
    // Flush all transports that support it
    const flushPromises = this.transports
      .filter((t) => t.flush)
      .map((t) => t.flush!());
    
    // Wait for all flushes, but don't fail if some fail
    await Promise.allSettled(flushPromises);
  }
}

// =============================================================================
// Type Augmentation for Tauri
// =============================================================================

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}
