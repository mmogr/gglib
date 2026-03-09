import { FC } from "react";
import { Circle, MessageCircle, RotateCcw, Square } from "lucide-react";
import { safeStopServer } from "../services/server/safeActions";
import { useProxyState } from "../services/proxyRegistry";
import { useAllServerStates } from "../services/serverRegistry";
import { Icon } from "./ui/Icon";
import { Stack } from './primitives';
import { cn } from "../utils/cn";
import { useToastContext } from '../contexts/ToastContext';

interface ServerStatusProps {
  onOpenChat?: (port: number, modelName: string) => void;
}

// Derive healthy status from server status
const isServerHealthy = (status: string): boolean => {
  return status === 'running' || status === 'healthy';
};

const ServerStatus: FC<ServerStatusProps> = ({ onOpenChat }) => {
  const serverStates = useAllServerStates();
  const proxyState = useProxyState();
  const { showToast } = useToastContext();

  const handleStopServer = async (modelId: number) => {
    try {
      await safeStopServer(modelId);
    } catch (err) {
      showToast(`Failed to stop server: ${err}`, 'error');
    }
  };

  // Don't show banner if nothing is running
  if (serverStates.length === 0 && !proxyState.running) {
    return null;
  }

  return (
    <div className="flex gap-base px-lg py-md bg-gradient-to-br from-primary to-[#764ba2] border-b border-white/10 flex-wrap items-center">
      {/* Proxy Status */}
      {proxyState.running && (
        <div className="flex items-center gap-sm px-md py-sm bg-white/10 rounded-md backdrop-blur-[10px] border border-white/20">
          <span className="text-xl leading-none" aria-hidden>
            <Icon icon={RotateCcw} size={16} />
          </span>
          <Stack gap="xs" className="text-white text-sm">
            <strong className="font-semibold">Proxy Active</strong>
            <span className="text-xs opacity-90">
              Port {proxyState.port}
            </span>
          </Stack>
        </div>
      )}

      {/* Running Servers */}
      {serverStates.map((server) => (
        <div key={server.modelId} className="flex items-center gap-sm px-md py-sm bg-white/10 rounded-md backdrop-blur-[10px]">
          <span className={cn('text-xl leading-none', isServerHealthy(server.status) && 'animate-pulse')} aria-hidden>
            <Icon icon={Circle} size={14} />
          </span>
          <Stack gap="xs" className="text-white text-sm">
            <strong className="font-semibold">{server.modelName ?? `Model ${server.modelId}`}</strong>
            <span className="text-xs opacity-90">Port {server.port}</span>
          </Stack>
          {isServerHealthy(server.status) && onOpenChat && (
            <button
              className="bg-white/20 border-none rounded-base px-sm py-xs cursor-pointer text-base transition-all text-white flex items-center justify-center hover:bg-white/30 hover:-translate-y-px active:translate-y-0"
              onClick={() => onOpenChat(server.port ?? 0, server.modelName ?? `Model ${server.modelId}`)}
              title="Open chat"
            >
              <Icon icon={MessageCircle} size={14} />
            </button>
          )}
          <button
            className="bg-white/20 border-none rounded-base px-sm py-xs cursor-pointer text-base transition-all ml-sm text-white hover:bg-white/30 hover:scale-110 active:scale-95"
            onClick={() => handleStopServer(Number(server.modelId))}
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
