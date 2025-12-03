import { useState, FC, useMemo } from "react";
import { GgufModel } from "../types";
import { removeModel, serveModel } from "../services/tauri";
import { formatParamCount } from "../utils/format";

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
      await removeModel(model.id.toString(), false);
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
          {loading ? "Loading..." : "🔄 Refresh"}
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
            <div key={model.id || model.name} className="table-row">
              <div className="cell model-name">
                <div className="name-primary">{model.name}</div>
                {model.hf_repo_id && (
                  <div className="name-secondary">📦 {model.hf_repo_id}</div>
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
                  onClick={() => handleServe(model)}
                  className="action-button serve-button"
                  title="Serve model"
                >
                  🚀
                </button>
                <button
                  onClick={() => handleRemove(model)}
                  className="action-button remove-button"
                  disabled={removing === model.id}
                  title="Remove model"
                >
                  {removing === model.id ? "..." : "🗑️"}
                </button>
              </div>
            </div>
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
                ✕
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
                      🧠 Reasoning
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
                      🔧 Agent
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
              <button 
                className="btn btn-secondary" 
                onClick={() => setServingModel(null)}
                disabled={isServing}
              >
                Cancel
              </button>
              <button 
                className={`btn btn-primary ${isServing ? 'btn-loading' : ''}`}
                onClick={handleConfirmServe}
                disabled={isServing}
              >
                {isServing ? (
                  <>
                    <span className="spinner"></span>
                    Loading model...
                  </>
                ) : (
                  'Start Server'
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ModelList;