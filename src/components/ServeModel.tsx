import { useState, FC, FormEvent } from "react";
import { GgufModel } from "../types";
import { TauriService } from "../services/tauri";

interface ServeModelProps {
  models: GgufModel[];
  onModelServed: () => void;
}

const ServeModel: FC<ServeModelProps> = ({ models, onModelServed }) => {
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [port, setPort] = useState("8080");
  const [ctxSize, setCtxSize] = useState("");
  const [mlock, setMlock] = useState(false);
  const [serving, setServing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    
    if (!selectedModelId) {
      setError("Please select a model to serve");
      return;
    }

    try {
      setServing(true);
      setError(null);
      
      await TauriService.serveModel({
        id: selectedModelId,
        ctx_size: ctxSize || undefined,
        mlock,
        port: parseInt(port, 10),
      });
      
      onModelServed();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to serve model");
    } finally {
      setServing(false);
    }
  };

  const getSelectedModel = () => {
    return models.find(m => m.id === selectedModelId);
  };

  return (
    <div className="serve-model-container">
      <h2>Serve Model</h2>
      <p className="description">
        Start a model server with llama-server. The server will be accessible via HTTP API.
      </p>

      {models.length === 0 ? (
        <div className="empty-state">
          <p>No models available. Add some models first!</p>
        </div>
      ) : (
        <form onSubmit={handleSubmit} className="serve-form">
          <div className="form-group">
            <label htmlFor="modelSelect">Select Model:</label>
            <select
              id="modelSelect"
              value={selectedModelId || ""}
              onChange={(e) => setSelectedModelId(Number(e.target.value) || null)}
              className="form-select"
              required
            >
              <option value="">Choose a model...</option>
              {models.map((model) => (
                <option key={model.id} value={model.id}>
                  {model.name} ({model.param_count_b}B - {model.quantization || "Unknown"})
                </option>
              ))}
            </select>
          </div>

          {getSelectedModel() && (
            <div className="model-info">
              <h3>Selected Model Info</h3>
              <div className="info-grid">
                <div className="info-item">
                  <strong>Architecture:</strong> {getSelectedModel()?.architecture || "Unknown"}
                </div>
                <div className="info-item">
                  <strong>Size:</strong> {getSelectedModel()?.param_count_b}B parameters
                </div>
                <div className="info-item">
                  <strong>Quantization:</strong> {getSelectedModel()?.quantization || "Unknown"}
                </div>
                <div className="info-item">
                  <strong>Context Length:</strong> {getSelectedModel()?.context_length || "Unknown"}
                </div>
              </div>
            </div>
          )}

          <div className="form-row">
            <div className="form-group">
              <label htmlFor="port">Port:</label>
              <input
                type="number"
                id="port"
                value={port}
                onChange={(e) => setPort(e.target.value)}
                min="1"
                max="65535"
                className="form-input"
                required
              />
            </div>

            <div className="form-group">
              <label htmlFor="ctxSize">Context Size (optional):</label>
              <input
                type="number"
                id="ctxSize"
                value={ctxSize}
                onChange={(e) => setCtxSize(e.target.value)}
                placeholder="Auto-detect"
                className="form-input"
              />
              <small className="form-hint">Leave empty to use model's default</small>
            </div>
          </div>

          <div className="form-group">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={mlock}
                onChange={(e) => setMlock(e.target.checked)}
                className="checkbox-input"
              />
              <span className="checkbox-text">Enable memory lock (mlock)</span>
            </label>
            <small className="form-hint">Prevents model from being paged to swap</small>
          </div>

          {error && <div className="error-message">{error}</div>}

          <div className="form-actions">
            <button
              type="submit"
              disabled={serving || !selectedModelId}
              className="btn btn-primary"
            >
              {serving ? "Starting Server..." : "Start Server"}
            </button>
          </div>
        </form>
      )}

      <div className="help-section">
        <h3>Server Information</h3>
        <ul>
          <li><strong>API Endpoint:</strong> http://localhost:{port}/v1/</li>
          <li><strong>Health Check:</strong> http://localhost:{port}/health</li>
          <li><strong>OpenAI Compatible:</strong> Yes, supports chat completions API</li>
          <li><strong>Stop Server:</strong> Close this application or terminate the process</li>
        </ul>
        
        <h3>Configuration Tips</h3>
        <ul>
          <li><strong>Context Size:</strong> Higher values allow longer conversations but use more memory</li>
          <li><strong>Memory Lock:</strong> Recommended for production to ensure consistent performance</li>
          <li><strong>Port Selection:</strong> Choose an unused port (default 8080 works well)</li>
        </ul>
      </div>
    </div>
  );
};

export default ServeModel;