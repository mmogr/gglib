import { FC, useEffect, useState } from "react";
import { Circle, MessageCircle, RotateCcw, Square } from "lucide-react";
import { appLogger } from '../services/platform';
import { listServers, getProxyStatus } from "../services/clients/servers";
import { safeStopServer } from "../services/server/safeActions";
import type { ServerInfo } from "../types";
import type { ProxyStatus } from "../services/transport/types/proxy";
import { Icon } from "./ui/Icon";
import { cn } from "../utils/cn";

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
    <div className="flex gap-spacing-base px-spacing-lg py-spacing-md bg-gradient-to-br from-primary to-[#764ba2] border-b border-white/10 flex-wrap items-center">
      {/* Proxy Status */}
      {proxyStatus && proxyStatus.running && (
        <div className="flex items-center gap-spacing-sm px-spacing-md py-spacing-sm bg-white/10 rounded-md backdrop-blur-[10px] border border-white/20">
          <span className="text-xl leading-none" aria-hidden>
            <Icon icon={RotateCcw} size={16} />
          </span>
          <div className="flex flex-col gap-spacing-xs text-white text-sm">
            <strong className="font-semibold">Proxy Active</strong>
            <span className="text-xs opacity-90">
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
        <div key={server.modelId} className="flex items-center gap-spacing-sm px-spacing-md py-spacing-sm bg-white/10 rounded-md backdrop-blur-[10px]">
          <span className={cn('text-xl leading-none', isServerHealthy(server) && 'animate-pulse')} aria-hidden>
            <Icon icon={Circle} size={14} />
          </span>
          <div className="flex flex-col gap-spacing-xs text-white text-sm">
            <strong className="font-semibold">{server.modelName}</strong>
            <span className="text-xs opacity-90">Port {server.port}</span>
          </div>
          {isServerHealthy(server) && onOpenChat && (
            <button
              className="bg-white/20 border-none rounded-base px-spacing-sm py-spacing-xs cursor-pointer text-base transition-all text-white flex items-center justify-center hover:bg-white/30 hover:-translate-y-px active:translate-y-0"
              onClick={() => onOpenChat(server.port, server.modelName)}
              title="Open chat"
            >
              <Icon icon={MessageCircle} size={14} />
            </button>
          )}
          <button
            className="bg-white/20 border-none rounded-base px-spacing-sm py-spacing-xs cursor-pointer text-base transition-all ml-spacing-sm text-white hover:bg-white/30 hover:scale-110 active:scale-95"
            onClick={() => handleStopServer(server.modelId)}
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
