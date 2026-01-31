import { FC, useEffect, useState } from "react";
import { Circle, MessageCircle, RotateCcw, Square } from "lucide-react";
import { appLogger } from '../services/platform';
import { listServers, getProxyStatus } from "../services/clients/servers";
import { safeStopServer } from "../services/server/safeActions";
import type { ServerInfo } from "../types";
import type { ProxyStatus } from "../services/transport/types/proxy";
import { Icon } from "./ui/Icon";
import styles from './ServerStatus.module.css';

interface ServerStatusProps {
  onOpenChat?: (port: number, modelName: string) => void;
}

// Derive healthy status from server status
const isServerHealthy = (server: ServerInfo): boolean => {
  return server.status === 'running' || server.status === 'healthy';
};

const ServerStatus: FC<ServerStatusProps> = ({ onOpenChat }) => {
  const [servers, setServers] = useState<ServerInfo[]>([]);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [loading, setLoading] = useState(true);

  const loadStatus = async () => {
    try {
      const serverList = await listServers();
      setServers(serverList);
      
      // Try to get proxy status
      try {
        const proxy = await getProxyStatus();
        setProxyStatus(proxy);
      } catch {
        setProxyStatus(null);
      }
    } catch (err) {
      appLogger.error('component.server', 'Failed to load server status', { error: err });
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
      await safeStopServer(modelId);
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
          <span className={styles.statusIcon} aria-hidden>
            <Icon icon={RotateCcw} size={16} />
          </span>
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
          <span className={`${styles.statusIcon} ${isServerHealthy(server) ? styles.healthy : ''}`} aria-hidden>
            <Icon icon={Circle} size={14} />
          </span>
          <div className={styles.statusInfo}>
            <strong>{server.model_name}</strong>
            <span className={styles.statusDetail}>Port {server.port}</span>
          </div>
          {isServerHealthy(server) && onOpenChat && (
            <button
              className={styles.chatButton}
              onClick={() => onOpenChat(server.port, server.model_name)}
              title="Open chat"
            >
              <Icon icon={MessageCircle} size={14} />
            </button>
          )}
          <button
            className={styles.stopButton}
            onClick={() => handleStopServer(server.model_id)}
            title="Stop server"
          >
            <Icon icon={Square} size={14} />
          </button>
        </div>
      ))}
    </div>
  );
};

export default ServerStatus;
