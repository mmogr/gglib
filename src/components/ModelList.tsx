import { useState, FC } from "react";
import { GgufModel } from "../types";
import { TauriService } from "../services/tauri";
import { formatParamCount } from "../utils/format";

interface ModelListProps {
  models: GgufModel[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onModelRemoved: () => void;
}

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
  const [isServing, setIsServing] = useState(false);

  const handleRemove = async (model: GgufModel) => {
    if (!model.id) return;
    
    const confirmed = window.confirm(`Are you sure you want to remove "${model.name}"?`);
    if (!confirmed) return;

    try {
      setRemoving(model.id);
      await TauriService.removeModel(model.id.toString(), false);
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
      
      await TauriService.serveModel({
        id: servingModel.id,
        context_length: contextLength,
        mlock: false,
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
        <div className="modal-overlay" onClick={() => !isServing && setServingModel(null)}>
          <div className="modal modal-md" onClick={(e) => e.stopPropagation()}>
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