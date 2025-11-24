import { FC, useEffect, useState } from "react";
import { TauriService } from "../services/tauri";
import styles from './ServerStatus.module.css';

interface ServerInfo {
  model_id: number;
  model_name: string;
  port: number;
  healthy: boolean;
}

interface ProxyStatus {
  running: boolean;
  port: number;
  current_model?: string;
  model_port?: number;
}

interface ServerStatusProps {
  onOpenChat?: (port: number, modelName: string) => void;
}

const ServerStatus: FC<ServerStatusProps> = ({ onOpenChat }) => {
  const [servers, setServers] = useState<ServerInfo[]>([]);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [loading, setLoading] = useState(true);

  const loadStatus = async () => {
    try {
      const serverList = await TauriService.listServers();
      setServers(serverList);
      
      // Try to get proxy status
      try {
        const proxy = await TauriService.getProxyStatus();
        setProxyStatus(proxy);
      } catch {
        setProxyStatus(null);
      }
    } catch (err) {
      console.error("Failed to load server status:", err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 3000); // Refresh every 3 seconds
    return () => clearInterval(interval);
  }, []);

  const handleStopServer = async (modelId: number) => {
    try {
      await TauriService.stopServer(modelId);
      await loadStatus();
    } catch (err) {
      alert(`Failed to stop server: ${err}`);
    }
  };

  if (loading) {
    return null; // Don't show anything while loading initially
  }

  // Don't show banner if nothing is running
  if (servers.length === 0 && (!proxyStatus || !proxyStatus.running)) {
    return null;
  }

  return (
    <div className={styles.banner}>
      {/* Proxy Status */}
      {proxyStatus && proxyStatus.running && (
        <div className={`${styles.statusItem} ${styles.proxyStatus}`}>
          <span className={styles.statusIcon}>🔄</span>
          <div className={styles.statusInfo}>
            <strong>Proxy Active</strong>
            <span className={styles.statusDetail}>
              Port {proxyStatus.port}
              {proxyStatus.current_model && (
                <> • {proxyStatus.current_model} on port {proxyStatus.model_port}</>
              )}
            </span>
          </div>
        </div>
      )}

      {/* Running Servers */}
      {servers.map((server) => (
        <div key={server.model_id} className={styles.statusItem}>
          <span className={`${styles.statusIcon} ${server.healthy ? styles.healthy : ''}`}>
            {server.healthy ? '🟢' : '🔴'}
          </span>
          <div className={styles.statusInfo}>
            <strong>{server.model_name}</strong>
            <span className={styles.statusDetail}>Port {server.port}</span>
          </div>
          {server.healthy && onOpenChat && (
            <button
              className={styles.chatButton}
              onClick={() => onOpenChat(server.port, server.model_name)}
              title="Open chat"
            >
              💬
            </button>
          )}
          <button
            className={styles.stopButton}
            onClick={() => handleStopServer(server.model_id)}
            title="Stop server"
          >
            ⏹️
          </button>
        </div>
      ))}
    </div>
  );
};

export default ServerStatus;
