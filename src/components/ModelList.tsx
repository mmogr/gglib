import { useState, FC, useMemo } from "react";
import { Brain, Package, Rocket, RotateCcw, Trash2, Wrench, X, Shield, CloudSync } from "lucide-react";
import { appLogger } from '../services/platform';
import { GgufModel } from "../types";
import { removeModel } from "../services/clients/models";
import { serveModel } from "../services/clients/servers";
import { formatParamCount } from "../utils/format";
import { TransportError, LlamaServerNotInstalledMetadata } from "../services/transport/errors";
import { LlamaInstallModal } from "./LlamaInstallModal";
import { ServerHealthIndicator } from "./ServerHealthIndicator";
import { VerificationModal } from "./VerificationModal";
import { useIsServerRunning } from "../services/serverRegistry";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";
import { Row } from "./primitives";
import { Input } from "./ui/Input";

interface ModelListProps {
  models: GgufModel[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onModelRemoved: () => void;
}

/** Check if model has a specific tag */
const hasTag = (model: GgufModel, tag: string): boolean => {
  return model.tags?.includes(tag) ?? false;
};

/** Check if model should auto-enable Jinja (has 'agent' or 'reasoning' tag) */
const shouldAutoEnableJinja = (model: GgufModel): boolean => {
  return hasTag(model, 'agent') || hasTag(model, 'reasoning');
};

/** Individual model row with health indicator */
interface ModelRowProps {
  model: GgufModel;
  removing: number | null;
  onServe: (model: GgufModel) => void;
  onRemove: (model: GgufModel) => void;
  onVerify: (model: GgufModel) => void;
  onCheckUpdates: (model: GgufModel) => void;
}

const ModelRow: FC<ModelRowProps> = ({ model, removing, onServe, onRemove, onVerify, onCheckUpdates }) => {
  const isRunning = useIsServerRunning(model.id ?? 0);

  return (
    <div className="table-row">
      <div className="cell font-medium text-base flex items-center gap-sm w-full break-words">
        <Row gap="sm" align="center" className="name-primary">
          {model.name}
          {isRunning && <ServerHealthIndicator modelId={model.id ?? 0} />}
        </Row>
        {model.hfRepoId && (
          <Row gap="xs" align="center" className="name-secondary">
            <Icon icon={Package} size={14} className="shrink-0" />
            <span>{model.hfRepoId}</span>
          </Row>
        )}
      </div>
      <div className="cell">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</div>
      <div className="cell">{model.architecture || "—"}</div>
      <div className="cell">
        <span className="py-xs px-sm bg-background rounded-sm text-xs font-medium text-primary border border-[rgba(59,130,246,0.3)]">
          {model.quantization || "—"}
        </span>
      </div>
      <div className="cell">{new Date(model.addedAt).toLocaleDateString()}</div>
      <div className="cell actions">
        <button
          onClick={() => onVerify(model)}
          className="action-button"
          title="Verify model integrity"
        >
          <Icon icon={Shield} size={16} />
        </button>
        <button
          onClick={() => onCheckUpdates(model)}
          className="action-button"
          title="Check for updates on HuggingFace"
          disabled={!model.hfRepoId}
        >
          <Icon icon={CloudSync} size={16} />
        </button>
        <button
          onClick={() => onServe(model)}
          className="action-button serve-button"
          title="Serve model"
        >
          <Icon icon={Rocket} size={16} />
        </button>
        <button
          onClick={() => onRemove(model)}
          className="action-button remove-button"
          disabled={removing === model.id}
          title="Remove model"
        >
          {removing === model.id ? "..." : <Icon icon={Trash2} size={16} />}
        </button>
      </div>
    </div>
  );
};

const ModelList: FC<ModelListProps> = ({
  models,
  loading,
  error,
  onRefresh,
  onModelRemoved,
}) => {
  const [removing, setRemoving] = useState<number | null>(null);
  const [servingModel, setServingModel] = useState<GgufModel | null>(null);
  const [verifyingModel, setVerifyingModel] = useState<GgufModel | null>(null);
  const [checkingUpdatesModel, setCheckingUpdatesModel] = useState<GgufModel | null>(null);
  const [customContext, setCustomContext] = useState<string>('');
  const [enableJinja, setEnableJinja] = useState<boolean>(false);
  const [isServing, setIsServing] = useState(false);
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [installMetadata, setInstallMetadata] = useState<LlamaServerNotInstalledMetadata | null>(null);

  // Auto-enable Jinja when serving model has agent/reasoning tags
  const jinjaAutoEnabled = useMemo(() => {
    return servingModel ? shouldAutoEnableJinja(servingModel) : false;
  }, [servingModel]);

  const handleRemove = async (model: GgufModel) => {
    if (!model.id) return;
    
    const confirmed = window.confirm(`Are you sure you want to remove "${model.name}"?`);
    if (!confirmed) return;

    try {
      setRemoving(model.id);
      await removeModel(model.id);
      onModelRemoved();
    } catch (err) {
      alert(`Failed to remove model: ${err}`);
    } finally {
      setRemoving(null);
    }
  };

  const handleServe = (model: GgufModel) => {
    if (!model.id) return;
    setServingModel(model);
    setCustomContext(''); // Reset custom context
    // Auto-enable Jinja for agent/reasoning models, reset for others
    setEnableJinja(shouldAutoEnableJinja(model));
  };

  const handleVerify = (model: GgufModel) => {
    if (!model.id) return;
    setVerifyingModel(model);
  };

  const handleCheckUpdates = (model: GgufModel) => {
    if (!model.id) return;
    setCheckingUpdatesModel(model);
  };

  const handleConfirmServe = async () => {
    if (!servingModel || !servingModel.id) return;
    
    setIsServing(true);
    try {
      // Determine context length: custom input > model default > undefined
      let contextLength: number | undefined = undefined;
      if (customContext.trim()) {
        const parsed = parseInt(customContext.trim());
        if (!isNaN(parsed) && parsed > 0) {
          contextLength = parsed;
        }
      } else if (servingModel.contextLength) {
        contextLength = servingModel.contextLength;
      }
      
      await serveModel({
        id: servingModel.id,
        contextLength: contextLength,
        mlock: false,
        jinja: enableJinja || jinjaAutoEnabled,
      });
      setServingModel(null);
      onRefresh(); // Refresh to show updated server status
    } catch (err) {
      appLogger.error('component.model', 'Failed to serve model', { error: err, modelId: servingModel?.id });
      
      // Check if this is a llama-server not installed error
      if (TransportError.isTransportError(err) && err.code === 'LLAMA_SERVER_NOT_INSTALLED') {
        const metadata = TransportError.getLlamaServerMetadata(err);
        if (metadata) {
          setInstallMetadata(metadata);
          setShowInstallModal(true);
          return; // Don't show generic alert
        }
      }
      
      alert(`Failed to serve model: ${err}`);
    } finally {
      setIsServing(false);
    }
  };



  if (error) {
    return (
      <div className="error-container">
        <p className="error-message">Error: {error}</p>
        <button onClick={onRefresh} className="retry-button">
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="model-list-container">
      <div className="list-header">
        <h2>Your Models ({models.length})</h2>
        <button onClick={onRefresh} className="text-lg" disabled={loading}>
          {loading ? "Loading..." : (
            <span className="inline-flex items-center gap-2">
              <Icon icon={RotateCcw} size={16} />
              Refresh
            </span>
          )}
        </button>
      </div>

      {loading && models.length === 0 ? (
        <div className="loading">Loading models...</div>
      ) : models.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-3xl px-xl text-center min-h-[300px]">
          <p>No models found. Add your first model to get started!</p>
        </div>
      ) : (
        <div className="flex flex-col w-full">
          <div className="table-header">
            <div className="header-cell">Name</div>
            <div className="header-cell">Size</div>
            <div className="header-cell">Architecture</div>
            <div className="header-cell">Quantization</div>
            <div className="header-cell">Added</div>
            <div className="header-cell">Actions</div>
          </div>
          {models.map((model) => (
            <ModelRow
              key={model.id || model.name}
              model={model}
              removing={removing}
              onServe={handleServe}
              onRemove={handleRemove}
              onVerify={handleVerify}
              onCheckUpdates={handleCheckUpdates}
            />
          ))}
        </div>
      )}

      {/* Serve Configuration Modal */}
      {servingModel && (
        <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && !isServing && setServingModel(null)}>
          <div className="modal modal-md">
            <div className="modal-header">
              <h3>Start Model Server</h3>
              <button 
                className="modal-close" 
                onClick={() => setServingModel(null)}
                disabled={isServing}
              >
                <Icon icon={X} size={14} />
              </button>
            </div>
            
            <div className="modal-body">
              <div className="flex justify-between items-center mb-lg p-base bg-background rounded-md border border-border">
                <strong>{servingModel.name}</strong>
                <span className="text-text-secondary text-sm">{formatParamCount(servingModel.paramCountB, servingModel.expertUsedCount, servingModel.expertCount)}</span>
              </div>
              
              {/* Capability Badges */}
              {(hasTag(servingModel, 'reasoning') || hasTag(servingModel, 'agent')) && (
                <Row gap="sm" className="capability-badges mb-4">
                  {hasTag(servingModel, 'reasoning') && (
                    <span 
                      className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-medium bg-[#f3e8ff] text-[#7c3aed]"
                      title="Model supports chain-of-thought reasoning with thinking tags"
                    >
                      <Icon icon={Brain} size={14} />
                      Reasoning
                    </span>
                  )}
                  {hasTag(servingModel, 'agent') && (
                    <span 
                      className="inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-medium bg-[#dbeafe] text-[#2563eb]"
                      title="Model supports tool/function calling for agentic workflows"
                    >
                      <Icon icon={Wrench} size={14} />
                      Agent
                    </span>
                  )}
                </Row>
              )}
              
              <div className="mb-lg">
                <label htmlFor="context-input" className="block mb-sm font-medium text-text">
                  Context Length
                  <span className="font-normal text-text-secondary text-sm"> (optional)</span>
                </label>
                <Input
                  id="context-input"
                  type="number"
                  className="w-full p-md bg-background-input border border-border rounded-base text-text text-base transition duration-200 focus:outline-none focus:border-border-focus focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)]"
                  placeholder={servingModel.contextLength ? `Default: ${servingModel.contextLength.toLocaleString()}` : 'Use model default'}
                  value={customContext}
                  onChange={(e) => setCustomContext(e.target.value)}
                  disabled={isServing}
                  min="1"
                />
                <p className="mt-sm text-sm text-text-secondary">
                  {servingModel.contextLength 
                    ? `Model's maximum: ${servingModel.contextLength.toLocaleString()} tokens`
                    : 'Leave empty to use model default'}
                </p>
              </div>

              {/* Jinja Templates Toggle */}
              <div className="mb-lg mt-4">
                <label 
                  htmlFor="jinja-toggle" 
                  className="flex items-center gap-2 cursor-pointer disabled:cursor-not-allowed"
                >
                  <input
                    id="jinja-toggle"
                    type="checkbox"
                    checked={enableJinja || jinjaAutoEnabled}
                    onChange={(e) => setEnableJinja(e.target.checked)}
                    disabled={isServing || jinjaAutoEnabled}
                  className="w-auto"
                  />
                  <span>Enable Jinja templates</span>
                  {jinjaAutoEnabled && (
                    <span className="text-xs text-[var(--color-text-secondary)] italic">
                      (auto-enabled for this model)
                    </span>
                  )}
                </label>
                <p className="mt-sm text-sm text-text-secondary mt-1">
                  Required for tool calling and advanced chat templates. {jinjaAutoEnabled ? 'Automatically enabled for agent/reasoning models.' : 'Enable if using function calling.'}
                </p>
              </div>
            </div>
            
            <div className="modal-footer">
              <Button 
                variant="secondary" 
                onClick={() => setServingModel(null)}
                disabled={isServing}
              >
                Cancel
              </Button>
              <Button 
                variant="primary"
                onClick={handleConfirmServe}
                isLoading={isServing}
              >
                {isServing ? 'Loading model...' : 'Start Server'}
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Llama Server Install Modal */}
      {showInstallModal && installMetadata && (
        <LlamaInstallModal
          metadata={installMetadata}
          onClose={() => {
            setShowInstallModal(false);
            setInstallMetadata(null);
          }}
          onInstalled={() => {
            setShowInstallModal(false);
            setInstallMetadata(null);
            // Retry serving after install
            if (servingModel) {
              handleConfirmServe();
            }
          }}
        />
      )}

      {/* Verification Modal */}
      {verifyingModel && verifyingModel.id && (
        <VerificationModal
          modelId={verifyingModel.id}
          modelName={verifyingModel.name}
          open={!!verifyingModel}
          onClose={() => setVerifyingModel(null)}
          mode="verify"
        />
      )}

      {/* Update Check Modal */}
      {checkingUpdatesModel && checkingUpdatesModel.id && (
        <VerificationModal
          modelId={checkingUpdatesModel.id}
          modelName={checkingUpdatesModel.name}
          open={!!checkingUpdatesModel}
          onClose={() => setCheckingUpdatesModel(null)}
          mode="update"
        />
      )}
    </div>
  );
};

export default ModelList;