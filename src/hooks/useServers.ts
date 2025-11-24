import { useState, useEffect, useCallback } from 'react';
import { TauriService } from '../services/tauri';
import { ServerInfo } from '../types';

export function useServers() {
  const [servers, setServers] = useState<ServerInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadServers = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const serverList = await TauriService.listServers();
      setServers(serverList);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(`Failed to load servers: ${errorMessage}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadServers();
    // Poll every 3 seconds
    const interval = setInterval(loadServers, 3000);
    return () => clearInterval(interval);
  }, [loadServers]);

  const stopServer = useCallback(async (modelId: number) => {
    await TauriService.stopServer(modelId);
    await loadServers();
  }, [loadServers]);

  return {
    servers,
    loading,
    error,
    loadServers,
    stopServer,
  };
}
