/**
 * Research Log Buffer Service
 *
 * A non-reactive logging service for deep research sessions.
 * Designed for performance - logs do NOT trigger React re-renders.
 *
 * Features:
 * - Multi-session support via Map<sessionId, LogEntry[]>
 * - In-memory buffering for UI export (download logs button)
 * - Delegates to AppLogger for unified file persistence
 * - Payload truncation to prevent memory bloat
 * - useSyncExternalStore-compatible subscription API
 */

import { appLogger } from './logging/appLogger';
import type { LogLevel } from './logging/types';

// =============================================================================
// Types
// =============================================================================

/**
 * Structured log entry for research sessions.
 */
export interface ResearchLogEntry {
  /** ISO timestamp */
  timestamp: string;
  /** Log level */
  level: LogLevel;
  /** Log category (e.g., 'runResearchLoop', 'factExtractor', 'LLMCaller') */
  category: string;
  /** Human-readable message */
  message: string;
  /** Session/research ID this log belongs to */
  sessionId: string;
  /** Optional structured data (will be truncated if too large) */
  data?: Record<string, unknown>;
}

/**
 * Session metadata tracked alongside logs.
 */
interface SessionMeta {
  /** When the session started */
  startedAt: string;
  /** Original query (truncated) */
  query: string;
}

// =============================================================================
// Configuration
// =============================================================================

/** Maximum string length for individual payload fields */
const MAX_PAYLOAD_STRING_LENGTH = 500;

/** Maximum entries to keep in memory per session */
const MAX_ENTRIES_PER_SESSION = 1000;

/** Maximum total sessions to track (LRU eviction) */
const MAX_SESSIONS = 10;

// =============================================================================
// Utilities
// =============================================================================

/**
 * Truncate a string to max length with ellipsis.
 */
export function truncateString(str: string, maxLength = MAX_PAYLOAD_STRING_LENGTH): string {
  if (str.length <= maxLength) return str;
  return str.slice(0, maxLength - 3) + '...';
}

/**
 * Deep truncate all string values in an object.
 * Arrays are capped at 10 items.
 */
export function truncatePayload(
  obj: unknown,
  maxStringLength = MAX_PAYLOAD_STRING_LENGTH,
  depth = 0
): unknown {
  // Prevent infinite recursion
  if (depth > 5) return '[max depth]';

  if (obj === null || obj === undefined) return obj;

  if (typeof obj === 'string') {
    return truncateString(obj, maxStringLength);
  }

  if (Array.isArray(obj)) {
    const truncatedArray = obj.slice(0, 10).map((item) => truncatePayload(item, maxStringLength, depth + 1));
    if (obj.length > 10) {
      truncatedArray.push(`[...${obj.length - 10} more items]`);
    }
    return truncatedArray;
  }

  if (typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      result[key] = truncatePayload(value, maxStringLength, depth + 1);
    }
    return result;
  }

  return obj;
}

// =============================================================================
// ResearchLogBuffer Singleton
// =============================================================================

type Listener = () => void;

/**
 * Singleton buffer for research logs.
 * Non-reactive - use subscribe() with useSyncExternalStore for React integration.
 */
class ResearchLogBuffer {
  /** Map of sessionId -> log entries */
  private sessions: Map<string, ResearchLogEntry[]> = new Map();

  /** Session metadata */
  private sessionMeta: Map<string, SessionMeta> = new Map();

  /** LRU order for session eviction */
  private sessionOrder: string[] = [];

  /** Subscribers for useSyncExternalStore */
  private listeners: Set<Listener> = new Set();

  /**
   * Start a new research session.
   * Call this at the beginning of each deep research run.
   */
  startSession(sessionId: string, query: string): void {
    // LRU eviction if at capacity
    if (this.sessions.size >= MAX_SESSIONS && !this.sessions.has(sessionId)) {
      const oldest = this.sessionOrder.shift();
      if (oldest) {
        this.sessions.delete(oldest);
        this.sessionMeta.delete(oldest);
      }
    }

    // Initialize session
    this.sessions.set(sessionId, []);
    this.sessionMeta.set(sessionId, {
      startedAt: new Date().toISOString(),
      query: truncateString(query, 200),
    });

    // Update LRU order
    this.sessionOrder = this.sessionOrder.filter((id) => id !== sessionId);
    this.sessionOrder.push(sessionId);

    // Log session start
    this.log(sessionId, 'info', 'session', `Research session started: ${truncateString(query, 100)}`);

    this.notifyListeners();
  }

  /**
   * End a research session.
   */
  endSession(sessionId: string): void {
    this.log(sessionId, 'info', 'session', 'Research session ended');
    this.notifyListeners();
  }

  /**
   * Log a message to a session.
   * Dual-storage: buffers in-memory for export + delegates to appLogger for persistence.
   */
  log(
    sessionId: string,
    level: LogLevel,
    category: string,
    message: string,
    data?: Record<string, unknown>
  ): void {
    const entry: ResearchLogEntry = {
      timestamp: new Date().toISOString(),
      level,
      category,
      message,
      sessionId,
      data: data ? (truncatePayload(data) as Record<string, unknown>) : undefined,
    };

    // 1. Buffer in-memory for UI/Export
    let logs = this.sessions.get(sessionId);
    if (!logs) {
      // Auto-create session if not exists (fallback)
      logs = [];
      this.sessions.set(sessionId, logs);
    }

    logs.push(entry);

    // Enforce max entries (drop oldest)
    if (logs.length > MAX_ENTRIES_PER_SESSION) {
      logs.shift();
    }

    // 2. Delegate to AppLogger for unified persistence
    // Map research category to appLogger's research.* categories
    const appLogCategory = category === 'session' ? 'research.session' : 'research.loop';
    appLogger.log(level, appLogCategory as any, message, { sessionId, ...data });
  }

  /**
   * Convenience methods for different log levels.
   */
  debug(sessionId: string, category: string, message: string, data?: Record<string, unknown>): void {
    this.log(sessionId, 'debug', category, message, data);
  }

  info(sessionId: string, category: string, message: string, data?: Record<string, unknown>): void {
    this.log(sessionId, 'info', category, message, data);
  }

  warn(sessionId: string, category: string, message: string, data?: Record<string, unknown>): void {
    this.log(sessionId, 'warn', category, message, data);
  }

  error(sessionId: string, category: string, message: string, data?: Record<string, unknown>): void {
    this.log(sessionId, 'error', category, message, data);
  }

  /**
   * Get all logs for a session.
   */
  getSessionLogs(sessionId: string): ResearchLogEntry[] {
    return this.sessions.get(sessionId) ?? [];
  }

  /**
   * Get session metadata.
   */
  getSessionMeta(sessionId: string): SessionMeta | undefined {
    return this.sessionMeta.get(sessionId);
  }

  /**
   * Get all active session IDs (for debugging).
   */
  getSessionIds(): string[] {
    return Array.from(this.sessions.keys());
  }

  /**
   * Export session logs as NDJSON string (for download).
   */
  exportSession(sessionId: string): string {
    const logs = this.sessions.get(sessionId) ?? [];
    return logs.map((entry) => JSON.stringify(entry)).join('\n');
  }

  /**
   * Export session logs as JSON array string.
   */
  exportSessionAsJSON(sessionId: string): string {
    const logs = this.sessions.get(sessionId) ?? [];
    return JSON.stringify(logs, null, 2);
  }

  /**
   * Clear logs for a session (keeps session registered).
   */
  clearSession(sessionId: string): void {
    this.sessions.set(sessionId, []);
    this.notifyListeners();
  }

  /**
   * Clear all sessions.
   */
  clearAll(): void {
    this.sessions.clear();
    this.sessionMeta.clear();
    this.sessionOrder = [];
    this.notifyListeners();
  }

  // ===========================================================================
  // useSyncExternalStore API
  // ===========================================================================

  /**
   * Subscribe to changes (for useSyncExternalStore).
   */
  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Get snapshot for a specific session (for useSyncExternalStore).
   * Returns the logs array reference - only changes when logs are added.
   */
  getSnapshot(sessionId: string): ResearchLogEntry[] {
    return this.sessions.get(sessionId) ?? [];
  }

  /**
   * Notify all listeners of changes.
   */
  private notifyListeners(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

// =============================================================================
// Singleton Export
// =============================================================================

/**
 * Global research log buffer instance.
 * Import this singleton and use its methods directly.
 */
export const researchLogger = new ResearchLogBuffer();
