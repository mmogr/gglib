import { useCallback } from 'react';
import { appLogger } from '../../../services/platform';
import type { GgufModel, ServeConfig, SparseInferenceConfig } from '../../../types';
import type { ServerViewModel } from '../../../hooks/useServers';
import { useToastContext } from '../../../contexts/ToastContext';
import { TransportError, LlamaServerNotInstalledMetadata } from '../../../services/transport/errors';
import { getTransport } from '../../../services/transport';
import { toStartServerRequest } from '../../../services/transport/mappers';
import { formatError } from '../../../utils/errors';

export interface ServerActionsConfig {
  model: GgufModel | null;
  // Serve modal state
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  hasAgentTag: boolean;
  mtpNMaxOverride: number | null;
  mtpPMinOverride: number | null;
  inferenceParams: SparseInferenceConfig | undefined;
  /** Serve as a pinned proxy instead of a bare model start. */
  pinProxy: boolean;
  onServerStarted?: (serverInfo: ServerViewModel) => void;
  onLlamaServerNotInstalled?: (metadata: LlamaServerNotInstalledMetadata) => void;
  setIsServing: (serving: boolean) => void;
  closeServeModal: () => void;
}

export interface ServerActionsResult {
  handleStartServer: () => Promise<void>;
}

/**
 * The Serve dialog's start (the inspector's "Load with options…" and "Load
 * and pin", `?serve=1` and the native menu's Serve), until the Load with
 * options sheet replaces the dialog (I2). Sends what the dialog's fields
 * hold and nothing resolved on the person's behalf.
 */
export function useServerActions(config: ServerActionsConfig): ServerActionsResult {
  const { showToast } = useToastContext();

  const {
    model,
    customContext,
    customPort,
    jinjaOverride,
    hasAgentTag,
    mtpNMaxOverride,
    mtpPMinOverride,
    inferenceParams,
    onServerStarted,
    onLlamaServerNotInstalled,
    setIsServing,
    closeServeModal,
    pinProxy,
  } = config;

  const handleStartServer = useCallback(async () => {
    if (!model?.id) return;

    setIsServing(true);
    try {
      // Only what the user typed, and nothing resolved on their behalf.
      //
      // This used to fall through to the stored default and then to
      // `model.contextLength`, the GGUF's *trained* window. Both landed on
      // the resolution ladder's top rung: the trained window outranked the
      // fit gglib computes from this machine's memory, so a model trained on
      // a window this machine cannot hold was handed one that will not load;
      // and the stored default outranked the model's own
      // `server_defaults.context_length` one rung below it. The daemon reads
      // that setting itself, so sending it bought nothing and cost that rung;
      // the trained window bought a launch that may not load. See ADR 0009.
      let contextLength: number | undefined = undefined;
      if (customContext.trim()) {
        const parsed = parseInt(customContext.trim(), 10);
        if (!isNaN(parsed) && parsed > 0) {
          contextLength = parsed;
        }
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
        // modal renders the whole `SamplingForm`, which offers
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
        mtpDraftNMax: mtpNMaxOverride ?? undefined,
        mtpDraftPMin: mtpPMinOverride ?? undefined,
      };

      if (pinProxy) {
        // The GUI's `gglib serve`: the daemon plans the pin server-side.
        // Send only what the user explicitly typed — pre-resolving context
        // client-side would bypass the cascade's model-defaults rung and
        // freeze the settings default as an explicit override. The shared
        // computation above now does exactly this, so the override is
        // belt-and-braces rather than a live correction; it stays because
        // this path is the one that must never regress into pre-resolving.
        const customCtx = customContext.trim() ? parseInt(customContext.trim(), 10) : NaN;
        try {
          const status = await getTransport().startPinnedProxy({
            model_id: model.id,
            options: {
              ...toStartServerRequest(serveConfig),
              contextLength: Number.isFinite(customCtx) && customCtx > 0 ? customCtx : undefined,
            },
          });
          showToast(
            `Proxy pinned to ${model.name}${status.port ? ` on port ${status.port}` : ''}`,
            'success',
          );
        } catch (err) {
          const raw = formatError(err);
          showToast(
            raw.includes('already running')
              ? 'The proxy is already running — stop it from the Proxy menu, then pin.'
              : `Could not pin the proxy: ${raw}`,
            'error',
          );
          return;
        }
        // The pin loads nothing: the proxy starts its model on the first
        // request, and chat needs a running server's port. So the model is
        // started below the ordinary way, under the pin, and chat opens on it.
      }

      const result = await getTransport().serveModel(serveConfig);
      closeServeModal();

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
      
      const errorMessage = formatError(error);
      if (errorMessage.toLowerCase().includes('port') && errorMessage.toLowerCase().includes('in use')) {
        showToast(errorMessage, 'error');
      } else {
        showToast(`Failed to start server: ${errorMessage}`, 'error');
      }
    } finally {
      setIsServing(false);
    }
  }, [model, customContext, customPort, jinjaOverride, hasAgentTag, mtpNMaxOverride, mtpPMinOverride, inferenceParams, onServerStarted, closeServeModal, setIsServing, showToast, onLlamaServerNotInstalled, pinProxy]);

  return { handleStartServer };
}
