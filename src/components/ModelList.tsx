import { useState, FC, useMemo } from "react";
import { Brain, Package, Rocket, RotateCcw, Trash2, Wrench, X } from "lucide-react";
import { GgufModel } from "../types";
import { removeModel } from "../services/clients/models";
import { serveModel } from "../services/clients/servers";
import { formatParamCount } from "../utils/format";
import { TransportError, LlamaServerNotInstalledMetadata } from "../services/transport/errors";
import { LlamaInstallModal } from "./LlamaInstallModal";
import { ServerHealthIndicator } from "./ServerHealthIndicator";
import { useIsServerRunning } from "../services/serverRegistry";
import { Icon } from "./ui/Icon";
import { Button } from "./ui/Button";

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
}

const ModelRow: FC<ModelRowProps> = ({ model, removing, onServe, onRemove }) => {
  const isRunning = useIsServerRunning(model.id ?? 0);

  return (
    <div className="table-row">
      <div className="cell model-name">
        <div className="name-primary" style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          {model.name}
          {isRunning && <ServerHealthIndicator modelId={model.id ?? 0} />}
        </div>
        {model.hf_repo_id && (
          <div className="name-secondary" style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}>
            <Icon icon={Package} size={14} className="shrink-0" />
            <span>{model.hf_repo_id}</span>
          </div>
        )}
      </div>
      <div className="cell">{formatParamCount(model.param_count_b)}</div>
      <div className="cell">{model.architecture || "—"}</div>
      <div className="cell">
        <span className="quantization-badge">
          {model.quantization || "—"}
        </span>
      </div>
      <div className="cell">{new Date(model.added_at).toLocaleDateString()}</div>
      <div className="cell actions">
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
      } else if (servingModel.context_length) {
        contextLength = servingModel.context_length;
      }
      
      await serveModel({
        id: servingModel.id,
        context_length: contextLength,
        mlock: false,
        jinja: enableJinja || jinjaAutoEnabled,
      });
      setServingModel(null);
      onRefresh(); // Refresh to show updated server status
    } catch (err) {
      console.error('Serve error:', err);
      
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
        <button onClick={onRefresh} className="refresh-button" disabled={loading}>
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
        <div className="empty-state">
          <p>No models found. Add your first model to get started!</p>
        </div>
      ) : (
        <div className="model-table">
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
              <div className="model-info">
                <strong>{servingModel.name}</strong>
                <span className="model-size">{formatParamCount(servingModel.param_count_b)}</span>
              </div>
              
              {/* Capability Badges */}
              {(hasTag(servingModel, 'reasoning') || hasTag(servingModel, 'agent')) && (
                <div className="capability-badges" style={{ display: 'flex', gap: '0.5rem', marginBottom: '1rem' }}>
                  {hasTag(servingModel, 'reasoning') && (
                    <span 
                      className="capability-badge reasoning" 
                      style={{ 
                        display: 'inline-flex', 
                        alignItems: 'center', 
                        gap: '0.25rem',
                        padding: '0.25rem 0.5rem',
                        borderRadius: '0.25rem',
                        backgroundColor: 'var(--color-purple-bg, #f3e8ff)',
                        color: 'var(--color-purple-text, #7c3aed)',
                        fontSize: '0.75rem',
                        fontWeight: 500
                      }}
                      title="Model supports chain-of-thought reasoning with thinking tags"
                    >
                      <Icon icon={Brain} size={14} />
                      Reasoning
                    </span>
                  )}
                  {hasTag(servingModel, 'agent') && (
                    <span 
                      className="capability-badge agent" 
                      style={{ 
                        display: 'inline-flex', 
                        alignItems: 'center', 
                        gap: '0.25rem',
                        padding: '0.25rem 0.5rem',
                        borderRadius: '0.25rem',
                        backgroundColor: 'var(--color-blue-bg, #dbeafe)',
                        color: 'var(--color-blue-text, #2563eb)',
                        fontSize: '0.75rem',
                        fontWeight: 500
                      }}
                      title="Model supports tool/function calling for agentic workflows"
                    >
                      <Icon icon={Wrench} size={14} />
                      Agent
                    </span>
                  )}
                </div>
              )}
              
              <div className="form-group">
                <label htmlFor="context-input">
                  Context Length
                  <span className="label-hint"> (optional)</span>
                </label>
                <input
                  id="context-input"
                  type="number"
                  className="context-input"
                  placeholder={servingModel.context_length ? `Default: ${servingModel.context_length.toLocaleString()}` : 'Use model default'}
                  value={customContext}
                  onChange={(e) => setCustomContext(e.target.value)}
                  disabled={isServing}
                  min="1"
                />
                <p className="input-help">
                  {servingModel.context_length 
                    ? `Model's maximum: ${servingModel.context_length.toLocaleString()} tokens`
                    : 'Leave empty to use model default'}
                </p>
              </div>

              {/* Jinja Templates Toggle */}
              <div className="form-group" style={{ marginTop: '1rem' }}>
                <label 
                  htmlFor="jinja-toggle" 
                  style={{ 
                    display: 'flex', 
                    alignItems: 'center', 
                    gap: '0.5rem',
                    cursor: isServing ? 'not-allowed' : 'pointer'
                  }}
                >
                  <input
                    id="jinja-toggle"
                    type="checkbox"
                    checked={enableJinja || jinjaAutoEnabled}
                    onChange={(e) => setEnableJinja(e.target.checked)}
                    disabled={isServing || jinjaAutoEnabled}
                    style={{ width: 'auto', margin: 0 }}
                  />
                  <span>Enable Jinja templates</span>
                  {jinjaAutoEnabled && (
                    <span 
                      style={{ 
                        fontSize: '0.75rem', 
                        color: 'var(--color-text-secondary, #6b7280)',
                        fontStyle: 'italic'
                      }}
                    >
                      (auto-enabled for this model)
                    </span>
                  )}
                </label>
                <p className="input-help" style={{ marginTop: '0.25rem' }}>
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
    </div>
  );
};

export default ModelList;