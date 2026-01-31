import { useState, useEffect, useCallback, useRef } from 'react';
import { 
  getServerLogs, 
  listenToServerLogs, 
  type ServerLogEntry,
  appLogger,
} from '../services/platform';

// Re-export type for consumers
export type { ServerLogEntry };

interface UseServerLogsOptions {
  serverPort: number;
  maxLines?: number;
}

interface UseServerLogsReturn {
  logs: ServerLogEntry[];
  clearLogs: () => void;
  isAutoScroll: boolean;
  setIsAutoScroll: (value: boolean) => void;
  copyAllLogs: () => void;
}

const DEFAULT_MAX_LINES = 5000;

/**
 * Hook to listen for server log events.
 * Uses platform abstraction for Tauri/Web parity.
 * Maintains a ring buffer of log entries for the specified server port.
 */
export function useServerLogs(options: UseServerLogsOptions): UseServerLogsReturn {
  const { serverPort, maxLines = DEFAULT_MAX_LINES } = options;
  
  const [logs, setLogs] = useState<ServerLogEntry[]>([]);
  const [isAutoScroll, setIsAutoScroll] = useState(true);
  const logsRef = useRef<ServerLogEntry[]>([]);

  // Keep ref in sync for use in callbacks
  useEffect(() => {
    logsRef.current = logs;
  }, [logs]);

  const addLogEntry = useCallback((entry: ServerLogEntry) => {
    // Only accept logs for our port
    if (entry.port !== serverPort) return;

    setLogs(prevLogs => {
      const newLogs = [...prevLogs, entry];
      // Trim to max lines (ring buffer behavior)
      if (newLogs.length > maxLines) {
        return newLogs.slice(newLogs.length - maxLines);
      }
      return newLogs;
    });
  }, [serverPort, maxLines]);

  const clearLogs = useCallback(() => {
    setLogs([]);
  }, []);

  const copyAllLogs = useCallback(() => {
    const text = logsRef.current.map(entry => entry.line).join('\n');
    navigator.clipboard.writeText(text).catch(err => {
      appLogger.error('hook.server', 'Failed to copy logs', { error: err });
    });
  }, []);

  // Fetch initial logs on mount
  useEffect(() => {
    getServerLogs(serverPort)
      .then(initialLogs => {
        if (initialLogs && initialLogs.length > 0) {
          setLogs(initialLogs);
        }
      })
      .catch(e => {
        appLogger.debug('hook.server', 'Could not fetch initial logs', { error: e });
      });
  }, [serverPort]);

  // Listen for log events
  useEffect(() => {
    let cleanup: (() => void) | null = null;

    listenToServerLogs(serverPort, addLogEntry)
      .then(unsubscribe => {
        cleanup = unsubscribe;
      })
      .catch(e => {
        appLogger.error('hook.server', 'Failed to setup listener', { error: e });
      });

    return () => {
      cleanup?.();
    };
  }, [serverPort, addLogEntry]);

  return {
    logs,
    clearLogs,
    isAutoScroll,
    setIsAutoScroll,
    copyAllLogs,
  };
}
