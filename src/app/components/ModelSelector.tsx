import { useState } from 'react';
import { ChevronDown, Play, RefreshCw, Square } from 'lucide-react';
import { toast } from 'sonner';

import type { Model, ServerStatus } from '../types/api';
import { api } from '../services/api';
import { Badge } from './ui/badge';
import { Button } from './ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu';

interface ModelSelectorProps {
  models: Model[];
  selectedModelId: number | null;
  onSelectModelId: (modelId: number | null) => void;
  serverStatus?: ServerStatus;
  onRefresh: () => void | Promise<void>;
}

export function ModelSelector({
  models,
  selectedModelId,
  onSelectModelId,
  serverStatus,
  onRefresh,
}: ModelSelectorProps) {
  const [loading, setLoading] = useState(false);

  const selectedModel = models.find((m) => (m.id ?? null) === selectedModelId) ?? null;

  const handleStartServer = async () => {
    if (!selectedModel?.id) return;
    setLoading(true);
    try {
      await api.startServer(selectedModel.id, selectedModel.context_length ?? undefined);
      toast.success('Server started');
      await onRefresh();
    } catch (error) {
      toast.error('Failed to start server');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const handleStopServer = async () => {
    if (!selectedModel?.id) return;
    setLoading(true);
    try {
      await api.stopServer(selectedModel.id);
      toast.success('Server stopped');
      await onRefresh();
    } catch (error) {
      toast.error('Failed to stop server');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const status = serverStatus?.status ?? 'stopped';
  const isRunning = status === 'running';
  const isStarting = status === 'starting';

  return (
    <div className="space-y-3">
      <div className="text-sm font-medium text-muted-foreground">Active Model</div>

      {selectedModel ? (
        <>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="outline" className="w-full justify-between">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="truncate">{selectedModel.name}</span>
                  {serverStatus && (
                    <Badge variant={isRunning ? 'default' : 'secondary'} className="shrink-0">
                      {status}
                    </Badge>
                  )}
                </div>
                <ChevronDown className="size-4 shrink-0 opacity-50" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent className="w-56">
              {models.map((model) => (
                <DropdownMenuItem
                  key={model.id ?? model.file_path}
                  onClick={() => onSelectModelId(model.id ?? null)}
                  className="flex flex-col items-start gap-1"
                >
                  <span className="font-medium">{model.name}</span>
                  <span className="text-xs text-muted-foreground">
                    {model.quantization ?? 'Unknown quant'}
                  </span>
                </DropdownMenuItem>
              ))}
              {models.length === 0 && <DropdownMenuItem disabled>No models available</DropdownMenuItem>}
            </DropdownMenuContent>
          </DropdownMenu>

          <div className="flex gap-2">
            {isRunning ? (
              <Button
                variant="destructive"
                className="flex-1"
                size="sm"
                onClick={() => void handleStopServer()}
                disabled={loading}
              >
                <Square className="size-4 mr-2" />
                Stop Server
              </Button>
            ) : (
              <Button
                variant="default"
                className="flex-1"
                size="sm"
                onClick={() => void handleStartServer()}
                disabled={loading || isStarting}
              >
                <Play className="size-4 mr-2" />
                {isStarting ? 'Starting...' : 'Start Server'}
              </Button>
            )}
            <Button variant="outline" size="sm" onClick={() => void onRefresh()} disabled={loading}>
              <RefreshCw className="size-4" />
            </Button>
          </div>
        </>
      ) : (
        <div className="text-sm text-muted-foreground text-center py-4">No model selected</div>
      )}
    </div>
  );
}
