import { useCallback } from 'react';
import { appLogger } from '../../../services/platform';
import type { GgufModel, ServeConfig, AppSettings, SparseInferenceConfig } from '../../../types';
import type { ServerViewModel } from '../../../hooks/useServers';
import { useToastContext } from '../../../contexts/ToastContext';
import { TransportError, LlamaServerNotInstalledMetadata } from '../../../services/transport/errors';
import { getTransport } from '../../../services/transport';
import { toStartServerRequest } from '../../../services/transport/mappers';
import { formatError } from '../../../utils/errors';

export interface ServerActionsConfig {
  model: GgufModel | null;
  settings: AppSettings | null;
  servers: ServerViewModel[];
  // Edit mode state
  editedName: string;
  editedQuantization: string;
  editedFilePath: string;
  editedInferenceDefaults: SparseInferenceConfig | undefined;
  // Serve modal state
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  hasAgentTag: boolean;
  hasMtpTag: boolean;
  mtpNMaxOverride: number | null;
  mtpPMinOverride: number | null;
  inferenceParams: SparseInferenceConfig | undefined;
  /** Serve as a pinned proxy instead of a bare model start. */
  pinProxy: boolean;
  editedServerDefaults: import('../../../types').ServerConfig | null | undefined;
  // Callbacks
  onStopServer: (modelId: number) => Promise<void>;
  onRemoveModel: (id: number, force: boolean) => void;
  onUpdateModel: (id: number, updates: { name?: string; quantization?: string; file_path?: string; inferenceDefaults?: SparseInferenceConfig; serverDefaults?: import('../../../types').ServerConfig | null }) => Promise<void>;
  onStartServer: () => void;
  onServerStarted?: (serverInfo: ServerViewModel) => void;
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
    editedInferenceDefaults,
    editedServerDefaults,
    customContext,
    customPort,
    jinjaOverride,
    hasAgentTag,
    hasMtpTag,
    mtpNMaxOverride,
    mtpPMinOverride,
    inferenceParams,
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
    pinProxy,
  } = config;

  const activeServer = model?.id ? servers.find(s => s.modelId === model.id) : undefined;
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
      } else if (settings?.defaultContextSize) {
        contextLength = settings.defaultContextSize;
      } else if (model.contextLength) {
        contextLength = model.contextLength;
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
        // The sampling half, spread rather than named field by field. The
        // modal renders the whole `InferenceParametersForm`, which offers
        // seventeen of the eighteen — every one but `seed`, which has no
        // control. Naming nine of them dropped eight the surface had just
        // offered, and `seed` along with them.
        //
        // First, so the launch fields below win. `ServeConfig` extends
        // `SparseInferenceConfig`, which shares no key with them, so this is
        // belt-and-braces rather than a live conflict — but the launch half is
        // computed from explicit modal state and should not be overridable by
        // whatever happens to be in the form's object.
        ...inferenceParams,
        id: model.id,
        contextLength: contextLength,
        port,
        mlock: false,
        jinja: jinjaOverride === null ? (hasAgentTag ? true : undefined) : jinjaOverride,
        // MTP: null = auto-detect from tag; 0 = disable; >0 = explicit token count
        specDraftNMax: mtpNMaxOverride !== null ? mtpNMaxOverride : (hasMtpTag ? undefined : undefined),
        specDraftPMin: mtpPMinOverride !== null ? mtpPMinOverride : undefined,
      };

      if (pinProxy) {
        // The GUI's `gglib serve`: the daemon plans the pin server-side.
        // Send only what the user explicitly typed — pre-resolving context
        // client-side would bypass the cascade's model-defaults rung and
        // freeze the settings default as an explicit override.
        const customCtx = customContext.trim() ? parseInt(customContext.trim(), 10) : NaN;
        try {
          const status = await getTransport().startPinnedProxy({
            model_id: model.id,
            options: {
              ...toStartServerRequest(serveConfig),
              contextLength: Number.isFinite(customCtx) && customCtx > 0 ? customCtx : undefined,
            },
          });
          closeServeModal();
          showToast(
            `Proxy pinned to ${model.name}${status.port ? ` on port ${status.port}` : ''}`,
            'success',
          );
          onStartServer();
        } catch (err) {
          const raw = err instanceof Error ? err.message : String(err);
          showToast(
            raw.includes('already running')
              ? 'The proxy is already running — stop it from the Proxy menu, then pin.'
              : `Could not pin the proxy: ${raw}`,
            'error',
          );
        }
        return;
      }

      const result = await getTransport().serveModel(serveConfig);
      closeServeModal();
      onStartServer();
      
      if (onServerStarted && result) {
        onServerStarted({
          modelId: model.id,
          modelName: model.name,
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
  }, [model, settings, customContext, customPort, jinjaOverride, hasAgentTag, hasMtpTag, mtpNMaxOverride, mtpPMinOverride, inferenceParams, onStartServer, onServerStarted, closeServeModal, setIsServing, showToast, onLlamaServerNotInstalled, pinProxy]);

  const handleToggleServer = useCallback(async () => {
    if (!model?.id) return;
    
    if (isRunning) {
      try {
        await onStopServer(model.id);
      } catch (error) {
        appLogger.error('hook.ui', 'Failed to stop server', { error, modelId: model.id });
        showToast(`Failed to stop server: ${formatError(error)}`, 'error');
      }
    }
  }, [model, isRunning, onStopServer, showToast]);

  const handleConfirmDelete = useCallback(async () => {
    if (!model?.id) return;
    setIsDeleting(true);
    try {
      await onRemoveModel(model.id, true);
      closeDeleteModal();
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to remove model', { error, modelId: model.id });
      showToast(`Failed to remove model: ${formatError(error)}`, 'error');
    } finally {
      setIsDeleting(false);
    }
  }, [model, onRemoveModel, closeDeleteModal, setIsDeleting, showToast]);

  const handleSave = useCallback(async () => {
    if (!model?.id) return;
    try {
      const updates: { name?: string; quantization?: string; filePath?: string; inferenceDefaults?: SparseInferenceConfig; serverDefaults?: import('../../../types').ServerConfig | null } = {};
      
      if (editedName !== model.name) {
        updates.name = editedName;
      }
      if (editedQuantization !== (model.quantization || '')) {
        updates.quantization = editedQuantization || undefined;
      }
      if (editedFilePath !== model.filePath) {
        updates.filePath = editedFilePath;
      }
      // Always include inferenceDefaults if it was edited (even if set to empty object to clear)
      if (JSON.stringify(editedInferenceDefaults) !== JSON.stringify(model.inferenceDefaults)) {
        updates.inferenceDefaults = editedInferenceDefaults;
      }
      // Include serverDefaults if it was edited (null = clear override, object = set value, undefined = no change)
      if (editedServerDefaults !== model.serverDefaults) {
        updates.serverDefaults = editedServerDefaults;
      }
      
      if (Object.keys(updates).length > 0) {
        await onUpdateModel(model.id, updates);
      }
      resetEditState();
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to update model', { error, modelId: model?.id });
      showToast(`Failed to update model: ${formatError(error)}`, 'error');
    }
  }, [model, editedName, editedQuantization, editedFilePath, editedInferenceDefaults, editedServerDefaults, onUpdateModel, resetEditState, showToast]);

  return {
    handleStartServer,
    handleToggleServer,
    handleConfirmDelete,
    handleSave,
    isRunning,
  };
}
