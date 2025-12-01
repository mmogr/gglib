import { useState, useEffect, useCallback, useRef } from 'react';
import { isTauriApp } from '../utils/platform';

export interface ServerLogEntry {
  timestamp: number;
  line: string;
  port: number;
}

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
 * Hook to listen for server log events from Tauri or Web SSE.
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
      console.error('Failed to copy logs:', err);
    });
  }, []);

  // Fetch initial logs on mount
  useEffect(() => {
    const fetchInitialLogs = async () => {
      if (isTauriApp) {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const initialLogs = await invoke<ServerLogEntry[]>('get_server_logs', { port: serverPort });
          if (initialLogs && initialLogs.length > 0) {
            setLogs(initialLogs);
          }
        } catch (e) {
          // Command may not exist yet or server has no logs
          console.debug('[useServerLogs] Could not fetch initial logs:', e);
        }
      } else {
        // Web mode: fetch from REST API
        try {
          const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
          const response = await fetch(`${baseUrl}/api/servers/${serverPort}/logs`);
          if (response.ok) {
            const initialLogs = await response.json() as ServerLogEntry[];
            if (initialLogs && initialLogs.length > 0) {
              setLogs(initialLogs);
            }
          }
        } catch (e) {
          console.debug('[useServerLogs] Could not fetch initial logs:', e);
        }
      }
    };

    fetchInitialLogs();
  }, [serverPort]);

  // Listen for log events
  useEffect(() => {
    let unlistenTauri: (() => void) | undefined;
    let eventSource: EventSource | undefined;

    const setupListener = async () => {
      if (isTauriApp) {
        try {
          const { listen } = await import('@tauri-apps/api/event');
          unlistenTauri = await listen<ServerLogEntry>('server-log', (event) => {
            addLogEntry(event.payload);
          });
        } catch (e) {
          console.error('[useServerLogs] Failed to setup Tauri listener:', e);
        }
      } else {
        // Web mode: use SSE
        const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
        eventSource = new EventSource(`${baseUrl}/api/servers/${serverPort}/logs/stream`);

        eventSource.onmessage = (event) => {
          try {
            if (!event.data || event.data.trim() === '') return;
            const logEntry = JSON.parse(event.data) as ServerLogEntry;
            addLogEntry(logEntry);
          } catch (e) {
            console.error('[useServerLogs] Failed to parse log event:', e);
          }
        };

        eventSource.onerror = (err) => {
          console.error('[useServerLogs] SSE Error:', err);
        };
      }
    };

    setupListener();

    return () => {
      if (unlistenTauri) {
        unlistenTauri();
      }
      if (eventSource) {
        eventSource.close();
      }
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
