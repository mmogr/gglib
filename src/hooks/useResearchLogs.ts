/**
 * useResearchLogs Hook
 *
 * React hook for subscribing to research log updates.
 * Uses useSyncExternalStore for optimal performance - only re-renders
 * when the subscribed session's logs change.
 *
 * @module hooks/useResearchLogs
 */

import { useSyncExternalStore, useCallback, useMemo } from 'react';
import { researchLogger, type ResearchLogEntry } from '../services/platform';

/**
 * Subscribe to logs for a specific research session.
 *
 * @param sessionId - The research session ID (usually messageId)
 * @returns The current logs array for this session
 *
 * @example
 * ```tsx
 * function ResearchDebugPanel({ sessionId }: { sessionId: string }) {
 *   const logs = useResearchLogs(sessionId);
 *
 *   return (
 *     <div>
 *       {logs.map((log, i) => (
 *         <div key={i}>[{log.level}] {log.message}</div>
 *       ))}
 *     </div>
 *   );
 * }
 * ```
 */
export function useResearchLogs(sessionId: string | undefined): ResearchLogEntry[] {
  // Memoize the subscribe function to prevent unnecessary re-subscriptions
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      return researchLogger.subscribe(onStoreChange);
    },
    []
  );

  // Memoize the getSnapshot function for this specific sessionId
  const getSnapshot = useCallback(() => {
    if (!sessionId) return [];
    return researchLogger.getSnapshot(sessionId);
  }, [sessionId]);

  // Server snapshot (same as client for this use case)
  const getServerSnapshot = getSnapshot;

  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
}

/**
 * Get all active session IDs.
 *
 * @returns Array of session IDs that have logs
 *
 * @example
 * ```tsx
 * function SessionSelector() {
 *   const sessionIds = useResearchSessionIds();
 *
 *   return (
 *     <select>
 *       {sessionIds.map(id => (
 *         <option key={id} value={id}>{id}</option>
 *       ))}
 *     </select>
 *   );
 * }
 * ```
 */
export function useResearchSessionIds(): string[] {
  const subscribe = useCallback(
    (onStoreChange: () => void) => {
      return researchLogger.subscribe(onStoreChange);
    },
    []
  );

  const getSnapshot = useCallback(() => {
    return researchLogger.getSessionIds();
  }, []);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/**
 * Hook to export logs for a session.
 *
 * @param sessionId - The research session ID
 * @returns Object with export functions
 *
 * @example
 * ```tsx
 * function ExportButton({ sessionId }: { sessionId: string }) {
 *   const { downloadAsJSON, downloadAsNDJSON } = useResearchLogExport(sessionId);
 *
 *   return (
 *     <button onClick={downloadAsJSON}>Download Logs</button>
 *   );
 * }
 * ```
 */
export function useResearchLogExport(sessionId: string | undefined) {
  const downloadAsNDJSON = useCallback(() => {
    if (!sessionId) return;

    const content = researchLogger.exportSession(sessionId);
    downloadBlob(content, `research-${sessionId}.ndjson`, 'application/x-ndjson');
  }, [sessionId]);

  const downloadAsJSON = useCallback(() => {
    if (!sessionId) return;

    const content = researchLogger.exportSessionAsJSON(sessionId);
    downloadBlob(content, `research-${sessionId}.json`, 'application/json');
  }, [sessionId]);

  const copyToClipboard = useCallback(async () => {
    if (!sessionId) return false;

    try {
      const content = researchLogger.exportSessionAsJSON(sessionId);
      await navigator.clipboard.writeText(content);
      return true;
    } catch {
      return false;
    }
  }, [sessionId]);

  return useMemo(
    () => ({
      downloadAsNDJSON,
      downloadAsJSON,
      copyToClipboard,
    }),
    [downloadAsNDJSON, downloadAsJSON, copyToClipboard]
  );
}

/**
 * Helper to download a string as a file.
 */
function downloadBlob(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);

  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.style.display = 'none';
  document.body.appendChild(a);
  a.click();

  // Cleanup
  setTimeout(() => {
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, 100);
}
