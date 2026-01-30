/**
 * Research Log Buffer Service
 *
 * A non-reactive logging service for deep research sessions.
 * Designed for performance - logs do NOT trigger React re-renders.
 *
 * Features:
 * - Multi-session support via Map<sessionId, LogEntry[]>
 * - Tauri file streaming (NDJSON format) for crash resilience
 * - Web fallback to in-memory only
 * - Payload truncation to prevent memory bloat
 * - useSyncExternalStore-compatible subscription API
 *
 * TRANSPORT_EXCEPTION: Uses Tauri file system for log persistence.
 * UI components should import from 'services/platform' rather than checking isTauriApp directly.
 */

import { isDesktop } from './detect';

// Type augmentation for Tauri internals
declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

// =============================================================================
// Types
// =============================================================================

/**
 * Log entry severity levels.
 */
export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

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
  /** Whether file streaming is active for this session */
  fileStreamActive: boolean;
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

/**
 * Format a log entry as NDJSON line.
 */
function toNDJSON(entry: ResearchLogEntry): string {
  return JSON.stringify(entry) + '\n';
}

// =============================================================================
// File Streaming (Tauri Only)
// =============================================================================

/**
 * Check if the Tauri invoke API is available.
 */
function hasTauriInvoke(): boolean {
  return !!(
    typeof window !== 'undefined' &&
    window.__TAURI_INTERNALS__ &&
    typeof window.__TAURI_INTERNALS__.invoke === 'function'
  );
}

/**
 * Invoke a Tauri command.
 */
async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Type guard ensures __TAURI_INTERNALS__ is defined when hasTauriInvoke() is true
  const tauriInternals = window.__TAURI_INTERNALS__!;
  return tauriInternals.invoke(cmd, args);
}

/**
 * Initialize file streaming via Tauri command.
 * Returns true if file streaming is available.
 */
async function initFileStreamTauri(): Promise<boolean> {
  if (!isDesktop() || !hasTauriInvoke()) return false;

  try {
    // Try to initialize the log directory via Tauri command
    await invokeTauri<void>('init_research_logs');
    return true;
  } catch (err) {
    // Command doesn't exist yet - file streaming not available
    console.warn('[researchLogger] Tauri file streaming not available (command not found):', err);
    return false;
  }
}

/**
 * Append a log entry to the session's log file via Tauri command.
 */
async function appendToFile(sessionId: string, entry: ResearchLogEntry): Promise<void> {
  if (!isDesktop() || !hasTauriInvoke()) return;

  try {
    const line = toNDJSON(entry);
    await invokeTauri<void>('append_research_log', {
      sessionId,
      line,
    });
  } catch {
    // Fail silently - don't break the research loop
    // Command may not exist yet
  }
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

  /** Whether file streaming has been initialized */
  private fileStreamInitialized = false;

  /**
   * Initialize file streaming (call once at app startup on desktop).
   */
  async initFileStream(): Promise<void> {
    if (this.fileStreamInitialized || !isDesktop()) return;

    const success = await initFileStreamTauri();
    this.fileStreamInitialized = success;

    if (success) {
      console.log('[researchLogger] File streaming initialized');
    } else {
      console.log('[researchLogger] File streaming not available, using in-memory only');
    }
  }

  /**
   * Start a new research session.
   * Call this at the beginning of each deep research run.
   */
  async startSession(sessionId: string, query: string): Promise<void> {
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
      fileStreamActive: this.fileStreamInitialized,
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

    // Add to in-memory buffer
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

    // Stream to file if enabled
    const meta = this.sessionMeta.get(sessionId);
    if (meta?.fileStreamActive) {
      // Fire and forget - don't await
      appendToFile(sessionId, entry).catch(() => {
        // Silently ignore file errors
      });
    }

    // Also output to console in dev mode
    if (import.meta.env.DEV) {
      const prefix = `[${category}]`;
      switch (level) {
        case 'debug':
          console.debug(prefix, message, data ?? '');
          break;
        case 'info':
          console.log(prefix, message, data ?? '');
          break;
        case 'warn':
          console.warn(prefix, message, data ?? '');
          break;
        case 'error':
          console.error(prefix, message, data ?? '');
          break;
      }
    }
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

/**
 * Initialize file streaming (call once at app startup).
 * No-op on web.
 */
export async function initResearchLogger(): Promise<void> {
  await researchLogger.initFileStream();
}
