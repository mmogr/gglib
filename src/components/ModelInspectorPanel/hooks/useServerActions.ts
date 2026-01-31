import { useCallback } from 'react';
import { appLogger } from '../../../services/platform';
import type { GgufModel, ServeConfig, ServerInfo, AppSettings } from '../../../types';
import { serveModel } from '../../../services/clients/servers';
import { useToastContext } from '../../../contexts/ToastContext';
import { TransportError, LlamaServerNotInstalledMetadata } from '../../../services/transport/errors';

export interface ServerActionsConfig {
  model: GgufModel | null;
  settings: AppSettings | null;
  servers: ServerInfo[];
  // Edit mode state
  editedName: string;
  editedQuantization: string;
  editedFilePath: string;
  // Serve modal state
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  hasAgentTag: boolean;
  // Callbacks
  onStopServer: (modelId: number) => Promise<void>;
  onRemoveModel: (id: number, force: boolean) => void;
  onUpdateModel: (id: number, updates: { name?: string; quantization?: string; file_path?: string }) => Promise<void>;
  onStartServer: () => void;
  onServerStarted?: (serverInfo: ServerInfo) => void;
  onLlamaServerNotInstalled?: (metadata: LlamaServerNotInstalledMetadata) => void;
  // State setters
  setIsServing: (serving: boolean) => void;
  setIsDeleting: (deleting: boolean) => void;
  closeServeModal: () => void;
  closeDeleteModal: () => void;
  resetEditState: () => void;
}

export interface ServerActionsResult {
  handleStartServer: () => Promise<void>;
  handleToggleServer: () => Promise<void>;
  handleConfirmDelete: () => Promise<void>;
  handleSave: () => Promise<void>;
  isRunning: boolean;
}

/**
 * Hook for server-related async actions.
 * Handles starting/stopping servers, deleting models, and saving edits.
 */
export function useServerActions(config: ServerActionsConfig): ServerActionsResult {
  const { showToast } = useToastContext();
  
  const {
    model,
    settings,
    servers,
    editedName,
    editedQuantization,
    editedFilePath,
    customContext,
    customPort,
    jinjaOverride,
    hasAgentTag,
    onStopServer,
    onRemoveModel,
    onUpdateModel,
    onStartServer,
    onServerStarted,
    onLlamaServerNotInstalled,
    setIsServing,
    setIsDeleting,
    closeServeModal,
    closeDeleteModal,
    resetEditState,
  } = config;

  const activeServer = model?.id ? servers.find(s => s.model_id === model.id) : undefined;
  const isRunning = !!activeServer;

  const handleStartServer = useCallback(async () => {
    if (!model?.id) return;

    setIsServing(true);
    try {
      // Priority: custom input > settings default > model metadata
      let contextLength: number | undefined = undefined;
      if (customContext.trim()) {
        const parsed = parseInt(customContext.trim());
        if (!isNaN(parsed) && parsed > 0) {
          contextLength = parsed;
        }
      } else if (settings?.default_context_size) {
        contextLength = settings.default_context_size;
      } else if (model.context_length) {
        contextLength = model.context_length;
      }

      // Parse port if specified (must be >= 1024)
      let port: number | undefined = undefined;
      if (customPort.trim()) {
        const parsed = parseInt(customPort.trim());
        if (!isNaN(parsed) && parsed >= 1024 && parsed <= 65535) {
          port = parsed;
        } else if (!isNaN(parsed) && parsed < 1024) {
          showToast('Port must be 1024 or higher (privileged ports require root)', 'error');
          setIsServing(false);
          return;
        }
      }

      const serveConfig: ServeConfig = {
        id: model.id,
        context_length: contextLength,
        port,
        mlock: false,
        jinja: jinjaOverride === null ? (hasAgentTag ? true : undefined) : jinjaOverride,
      };

      const result = await serveModel(serveConfig);
      closeServeModal();
      onStartServer();
      
      if (onServerStarted && result) {
        onServerStarted({
          model_id: model.id,
          model_name: model.name,
          port: result.port,
          status: 'running',
        });
      }
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to start server', { error, modelId: model?.id });
      
      // Check if this is a llama-server not installed error
      if (TransportError.isTransportError(error) && error.code === 'LLAMA_SERVER_NOT_INSTALLED') {
        const metadata = TransportError.getLlamaServerMetadata(error);
        if (metadata && onLlamaServerNotInstalled) {
          closeServeModal();
          onLlamaServerNotInstalled(metadata);
          return; // Don't show generic toast
        }
      }
      
      const errorMessage = error instanceof Error ? error.message : String(error);
      if (errorMessage.toLowerCase().includes('port') && errorMessage.toLowerCase().includes('in use')) {
        showToast(errorMessage, 'error');
      } else {
        showToast(`Failed to start server: ${errorMessage}`, 'error');
      }
    } finally {
      setIsServing(false);
    }
  }, [model, settings, customContext, customPort, jinjaOverride, hasAgentTag, onStartServer, onServerStarted, closeServeModal, setIsServing, showToast, onLlamaServerNotInstalled]);

  const handleToggleServer = useCallback(async () => {
    if (!model?.id) return;
    
    if (isRunning) {
      try {
        await onStopServer(model.id);
      } catch (error) {
        appLogger.error('hook.ui', 'Failed to stop server', { error, modelId: model.id });
        alert(`Failed to stop server: ${error}`);
      }
    }
  }, [model, isRunning, onStopServer]);

  const handleConfirmDelete = useCallback(async () => {
    if (!model?.id) return;
    setIsDeleting(true);
    try {
      await onRemoveModel(model.id, true);
      closeDeleteModal();
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to remove model', { error, modelId: model.id });
      alert(`Failed to remove model: ${error}`);
    } finally {
      setIsDeleting(false);
    }
  }, [model, onRemoveModel, closeDeleteModal, setIsDeleting]);

  const handleSave = useCallback(async () => {
    if (!model?.id) return;
    try {
      const updates: { name?: string; quantization?: string; file_path?: string } = {};
      
      if (editedName !== model.name) {
        updates.name = editedName;
      }
      if (editedQuantization !== (model.quantization || '')) {
        updates.quantization = editedQuantization || undefined;
      }
      if (editedFilePath !== model.file_path) {
        updates.file_path = editedFilePath;
      }
      
      if (Object.keys(updates).length > 0) {
        await onUpdateModel(model.id, updates);
      }
      resetEditState();
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to update model', { error, modelId: model?.id });
      alert(`Failed to update model: ${error}`);
    }
  }, [model, editedName, editedQuantization, editedFilePath, onUpdateModel, resetEditState]);

  return {
    handleStartServer,
    handleToggleServer,
    handleConfirmDelete,
    handleSave,
    isRunning,
  };
}
