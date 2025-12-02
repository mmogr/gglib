import { FC, useState, useEffect, useCallback } from 'react';
import { GgufModel, ServeConfig, ServerInfo, HfModelSummary, DownloadQueueStatus } from '../../types';
import { TauriService } from '../../services/tauri';
import { useSettings } from '../../hooks/useSettings';
import { useToast } from '../../hooks/useToast';
import { formatParamCount, getHuggingFaceUrl } from '../../utils/format';
import { HfModelPreview } from '../HfModelPreview';
import './ModelInspectorPanel.css';

interface ModelInspectorPanelProps {
  model: GgufModel | null;
  /** Selected HuggingFace model for preview (mutually exclusive with local model) */
  selectedHfModel?: HfModelSummary | null;
  onStartServer: () => void;
  onServerStarted?: (serverInfo: ServerInfo) => void;
  onStopServer: (modelId: number) => Promise<void>;
  servers: ServerInfo[];
  onRemoveModel: (id: number, force: boolean) => void;
  onUpdateModel: (id: number, updates: {
    name?: string;
    quantization?: string;
    file_path?: string;
  }) => Promise<void>;
  onAddTag: (modelId: number, tag: string) => Promise<void>;
  onRemoveTag: (modelId: number, tag: string) => Promise<void>;
  getModelTags: (modelId: number) => Promise<string[]>;
  /** Callback to refresh model list after tag changes */
  onRefresh?: () => Promise<void>;
  /** Queue status from parent - for checking if downloads are disabled */
  queueStatus?: DownloadQueueStatus | null;
}

const ModelInspectorPanel: FC<ModelInspectorPanelProps> = ({
  model,
  selectedHfModel,
  onStartServer,
  onServerStarted,
  onStopServer,
  servers,
  onRemoveModel,
  onUpdateModel,
  onAddTag,
  onRemoveTag,
  getModelTags,
  onRefresh,
  queueStatus,
}) => {
  const { settings } = useSettings();
  const { showToast } = useToast();
  const [modelTags, setModelTags] = useState<string[]>([]);
  const [showServeModal, setShowServeModal] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isEditMode, setIsEditMode] = useState(false);
  const [editedName, setEditedName] = useState('');
  const [editedQuantization, setEditedQuantization] = useState('');
  const [editedFilePath, setEditedFilePath] = useState('');
  const [customContext, setCustomContext] = useState('');
  const [customPort, setCustomPort] = useState('');
  const [jinjaOverride, setJinjaOverride] = useState<boolean | null>(null);
  const [isServing, setIsServing] = useState(false);

  // Download handler for HF models - uses queue to support multiple downloads
  const handleHfDownload = useCallback(async (modelId: string, quantization: string) => {
    try {
      await TauriService.queueDownload(modelId, quantization);
    } catch (error) {
      console.error('Failed to start download:', error);
    }
  }, []);

  // Check if download queue is full (using queueStatus from parent)
  const maxQueueSize = queueStatus?.max_size ?? 3;
  const currentQueueCount = (queueStatus?.current ? 1 : 0) + (queueStatus?.pending?.length ?? 0);
  const downloadsDisabled = currentQueueCount >= maxQueueSize;
  const disabledReason = downloadsDisabled 
    ? `Download queue is full (${currentQueueCount}/${maxQueueSize})`
    : undefined;
  const [isDeleting, setIsDeleting] = useState(false);
  const [newTag, setNewTag] = useState('');

  const activeServer = model?.id ? servers.find(s => s.model_id === model.id) : undefined;
  const isRunning = !!activeServer;

  useEffect(() => {
    if (model?.id) {
      loadModelTags();
    } else {
      setModelTags([]);
    }
  }, [model?.id]);

  useEffect(() => {
    setJinjaOverride(null);
  }, [model?.id]);

  const loadModelTags = async () => {
    if (!model?.id) return;
    try {
      const tags = await getModelTags(model.id);
      setModelTags(tags);
    } catch (error) {
      console.error('Failed to load model tags:', error);
    }
  };

  const handleAddTag = async () => {
    if (!model?.id || !newTag.trim()) return;
    try {
      await onAddTag(model.id, newTag.trim());
      await loadModelTags();
      await onRefresh?.(); // Refresh model list so filtering uses updated tags
      setNewTag('');
    } catch (error) {
      console.error('Failed to add tag:', error);
    }
  };

  const handleRemoveTag = async (tag: string) => {
    if (!model?.id) return;
    try {
      await onRemoveTag(model.id, tag);
      await loadModelTags();
      await onRefresh?.(); // Refresh model list so filtering uses updated tags
    } catch (error) {
      console.error('Failed to remove tag:', error);
    }
  };

  const handleToggleServer = async () => {
    if (!model?.id) return;
    
    if (isRunning) {
      try {
        await onStopServer(model.id);
      } catch (error) {
        console.error('Failed to stop server:', error);
        alert(`Failed to stop server: ${error}`);
      }
    } else {
      setJinjaOverride(null);
      setShowServeModal(true);
    }
  };

  const handleStartServer = async () => {
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

      const config: ServeConfig = {
        id: model.id,
        context_length: contextLength,
        port,
        mlock: false,
        jinja: jinjaOverride === null ? undefined : jinjaOverride,
      };

      const result = await TauriService.serveModel(config);
      setShowServeModal(false);
      setCustomPort(''); // Reset port input for next time
      onStartServer();
      
      // Notify parent that server started - opens chat view
      if (onServerStarted && result) {
        onServerStarted({
          model_id: model.id,
          model_name: model.name,
          port: result.port,
          status: 'running',
        });
      }
    } catch (error) {
      console.error('Failed to start server:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      // Check if it's a port-related error for better UX
      if (errorMessage.toLowerCase().includes('port') && errorMessage.toLowerCase().includes('in use')) {
        showToast(errorMessage, 'error');
      } else {
        showToast(`Failed to start server: ${errorMessage}`, 'error');
      }
    } finally {
      setIsServing(false);
    }
  };

  const handleRemove = async () => {
    if (!model?.id) return;
    setShowDeleteModal(true);
  };

  const handleConfirmDelete = async () => {
    if (!model?.id) return;
    setIsDeleting(true);
    try {
      // Pass true for force since we already confirmed in the GUI modal
      await onRemoveModel(model.id, true);
      setShowDeleteModal(false);
    } catch (error) {
      console.error('Failed to remove model:', error);
      alert(`Failed to remove model: ${error}`);
    } finally {
      setIsDeleting(false);
    }
  };

  const handleEdit = () => {
    if (!model) return;
    setEditedName(model.name);
    setEditedQuantization(model.quantization || '');
    setEditedFilePath(model.file_path);
    setIsEditMode(true);
  };

  const handleSave = async () => {
    if (!model?.id) return;
    try {
      const updates: {
        name?: string;
        quantization?: string;
        file_path?: string;
      } = {};
      
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
      setIsEditMode(false);
    } catch (error) {
      console.error('Failed to update model:', error);
      alert(`Failed to update model: ${error}`);
    }
  };

  const handleCancel = () => {
    setIsEditMode(false);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  const combinedTags = modelTags.length > 0 ? modelTags : (model?.tags || []);
  const hasAgentTag = combinedTags.some(tag => tag.toLowerCase() === 'agent');
  const effectiveJinjaEnabled = jinjaOverride === null ? hasAgentTag : jinjaOverride;
  const isAutoJinja = jinjaOverride === null && hasAgentTag;

  // If HuggingFace model is selected, show HF model preview with download options
  if (selectedHfModel) {
    return (
      <div className="mcc-panel inspector-panel hf-preview-panel">
        {/* HF Model Preview - download progress is now handled by GlobalDownloadStatus at page level */}
        <HfModelPreview
          model={selectedHfModel}
          onDownload={handleHfDownload}
          downloadsDisabled={downloadsDisabled}
          disabledReason={disabledReason}
        />
      </div>
    );
  }

  if (!model) {
    return (
      <div className="mcc-panel inspector-panel">
        <div className="mcc-panel-content">
          <div className="inspector-empty">
            <div className="empty-icon">👈</div>
            <p>Select a model to view details</p>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="mcc-panel inspector-panel">
      <div className="mcc-panel-header">
        {isEditMode ? (
          <input
            type="text"
            className="inspector-title-edit"
            value={editedName}
            onChange={(e) => setEditedName(e.target.value)}
            placeholder="Model name"
          />
        ) : (
          <h2 className="inspector-title">{model.name}</h2>
        )}
      </div>

      <div className="mcc-panel-content">
        <div className="inspector-content">
          {/* Metadata Section */}
          <section className="inspector-section">
            <h3>Model Information</h3>
            <div className="metadata-grid">
              <div className="metadata-row">
                <span className="metadata-label">Size:</span>
                <span className="metadata-value">{formatParamCount(model.param_count_b)}</span>
              </div>
              {model.architecture && (
                <div className="metadata-row">
                  <span className="metadata-label">Architecture:</span>
                  <span className="metadata-value">{model.architecture}</span>
                </div>
              )}
              {(model.quantization || isEditMode) && (
                <div className="metadata-row">
                  <span className="metadata-label">Quantization:</span>
                  {isEditMode ? (
                    <input
                      type="text"
                      className="metadata-value-edit"
                      value={editedQuantization}
                      onChange={(e) => setEditedQuantization(e.target.value)}
                      placeholder="e.g., Q4_0"
                    />
                  ) : (
                    <span className="metadata-value quantization">{model.quantization}</span>
                  )}
                </div>
              )}
              {model.context_length && (
                <div className="metadata-row">
                  <span className="metadata-label">Context Length:</span>
                  <span className="metadata-value">{model.context_length.toLocaleString()}</span>
                </div>
              )}
              <div className="metadata-row">
                <span className="metadata-label">Path:</span>
                {isEditMode ? (
                  <input
                    type="text"
                    className="metadata-value-edit path-edit"
                    value={editedFilePath}
                    onChange={(e) => setEditedFilePath(e.target.value)}
                    placeholder="File path"
                  />
                ) : (
                  <span className="metadata-value path">
                    {model.file_path}
                    <button
                      className="icon-btn icon-btn-sm"
                      onClick={() => copyToClipboard(model.file_path)}
                      title="Copy path"
                    >
                      📋
                    </button>
                  </span>
                )}
              </div>
              {model.hf_repo_id && (
                <div className="metadata-row">
                  <span className="metadata-label">HuggingFace:</span>
                  <span className="metadata-value hf-link-container">
                    <span className="hf-repo-id">{model.hf_repo_id}</span>
                    <button
                      className="hf-link-button"
                      onClick={() => {
                        const url = getHuggingFaceUrl(model.hf_repo_id);
                        if (url) TauriService.openUrl(url);
                      }}
                      title="Open on HuggingFace"
                      aria-label="Open on HuggingFace"
                    >
                      🤗
                    </button>
                  </span>
                </div>
              )}
            </div>
          </section>

          {/* Tags Section */}
          <section className="inspector-section">
            <h3>Tags</h3>
            <div className="tags-container">
              {modelTags.length === 0 ? (
                <p className="text-muted">No tags assigned</p>
              ) : (
                <div className="tag-chips">
                  {modelTags.map(tag => (
                    <div
                      key={tag}
                      className="tag-chip"
                    >
                      {tag}
                      <button
                        className="tag-remove"
                        onClick={() => handleRemoveTag(tag)}
                        title="Remove tag"
                      >
                        ×
                      </button>
                    </div>
                  ))}
                </div>
              )}
              <div className="tag-add-dropdown">
                <input
                  type="text"
                  className="tag-select"
                  placeholder="Add tag..."
                  value={newTag}
                  onChange={(e) => setNewTag(e.target.value)}
                  onKeyPress={(e) => {
                    if (e.key === 'Enter') {
                      handleAddTag();
                    }
                  }}
                />
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={handleAddTag}
                  disabled={!newTag.trim()}
                >
                  Add
                </button>
              </div>
            </div>
          </section>

          {/* Actions Section */}
          <section className="inspector-section actions-section">
            <button 
              className={`btn btn-lg ${isRunning ? 'btn-danger' : 'btn-primary'}`}
              onClick={handleToggleServer}
              disabled={isEditMode}
            >
              {isRunning ? '⏹️ Stop Endpoint' : '🚀 Start Endpoint'}
            </button>
            <div className="secondary-actions">
              {isEditMode ? (
                <>
                  <button className="btn btn-primary" onClick={handleSave}>
                    ✓ Save
                  </button>
                  <button className="btn btn-secondary" onClick={handleCancel}>
                    ✕ Cancel
                  </button>
                </>
              ) : (
                <>
                  <button className="btn btn-secondary" onClick={handleEdit}>
                    ✏️ Edit
                  </button>
                  <button className="btn btn-secondary" onClick={handleRemove}>
                    🗑️ Delete
                  </button>
                </>
              )}
            </div>
          </section>
        </div>
      </div>

      {/* Serve Modal */}
      {showServeModal && (
        <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && !isServing && setShowServeModal(false)}>
          <div className="modal modal-md">
            <div className="modal-header">
              <h3>Start Model Server</h3>
              <button
                className="modal-close"
                onClick={() => setShowServeModal(false)}
                disabled={isServing}
              >
                ✕
              </button>
            </div>

            <div className="modal-body">
              <div className="model-info">
                <strong>{model.name}</strong>
                <span className="model-size">{formatParamCount(model.param_count_b)}</span>
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
                  placeholder={
                    settings?.default_context_size
                      ? `Default: ${settings.default_context_size.toLocaleString()}`
                      : model.context_length
                        ? `Model max: ${model.context_length.toLocaleString()}`
                        : 'Enter context length'
                  }
                  value={customContext}
                  onChange={(e) => setCustomContext(e.target.value)}
                  disabled={isServing}
                  min="1"
                />
                <p className="input-help">
                  {model.context_length
                    ? `Model's maximum: ${model.context_length.toLocaleString()} tokens`
                    : 'No model context metadata available'}
                </p>
              </div>

              <div className="form-group">
                <label htmlFor="port-input">
                  Port
                  <span className="label-hint"> (optional)</span>
                </label>
                <input
                  id="port-input"
                  type="number"
                  className="context-input"
                  placeholder={
                    settings?.server_port
                      ? `Auto (from ${settings.server_port})`
                      : 'Auto (from 9000)'
                  }
                  value={customPort}
                  onChange={(e) => setCustomPort(e.target.value)}
                  disabled={isServing}
                  min="1024"
                  max="65535"
                />
                <p className="input-help">
                  Leave empty to auto-allocate from base port
                </p>
              </div>

              {hasAgentTag && (
                <div className="jinja-alert" role="status">
                  <div className="jinja-alert-title">Agent tag detected</div>
                  <p>
                    Jinja templates {jinjaOverride === false ? 'would normally be auto-enabled for agent-tagged models, but you have disabled them for this launch.' : 'will be enabled automatically for agent-tagged models to support structured prompts.'}
                  </p>
                  {jinjaOverride !== null && (
                    <button
                      type="button"
                      className="btn btn-link btn-sm"
                      onClick={() => setJinjaOverride(null)}
                      disabled={isServing}
                    >
                      Reset to auto-detect
                    </button>
                  )}
                </div>
              )}

              <div className="form-group">
                <div className="form-label-row">
                  <label htmlFor="jinja-toggle">Jinja Templates</label>
                  <span className="jinja-mode-label">
                    {isAutoJinja
                      ? 'Auto (agent tag)'
                      : (jinjaOverride === null
                        ? 'Disabled'
                        : (jinjaOverride ? 'Enabled manually' : 'Disabled manually'))}
                  </span>
                </div>
                <div className="jinja-toggle-row">
                  <input
                    id="jinja-toggle"
                    type="checkbox"
                    checked={effectiveJinjaEnabled}
                    onChange={(e) => setJinjaOverride(e.target.checked)}
                    disabled={isServing}
                  />
                  <div className="jinja-toggle-copy">
                    <p>
                      Enable llama.cpp's Jinja templating for instruction/agent models. Leave off for plain chat models.
                    </p>
                  </div>
                </div>
              </div>
            </div>

            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setShowServeModal(false)}
                disabled={isServing}
              >
                Cancel
              </button>
              <button
                className={`btn btn-primary ${isServing ? 'btn-loading' : ''}`}
                onClick={handleStartServer}
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

      {/* Delete Confirmation Modal */}
      {showDeleteModal && (
        <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && !isDeleting && setShowDeleteModal(false)}>
          <div className="modal modal-sm">
            <div className="modal-header">
              <h3>Delete Model</h3>
              <button
                className="modal-close"
                onClick={() => setShowDeleteModal(false)}
                disabled={isDeleting}
              >
                ✕
              </button>
            </div>

            <div className="modal-body">
              <p>Are you sure you want to remove <strong>"{model.name}"</strong> from the database?</p>
              <p className="text-muted" style={{ marginTop: 'var(--spacing-base)' }}>
                Note: The model file will remain on disk and won't be deleted.
              </p>
            </div>

            <div className="modal-footer">
              <button
                className="btn btn-secondary"
                onClick={() => setShowDeleteModal(false)}
                disabled={isDeleting}
              >
                Cancel
              </button>
              <button
                className="btn btn-danger"
                onClick={handleConfirmDelete}
                disabled={isDeleting}
              >
                {isDeleting ? (
                  <>
                    <span className="spinner"></span>
                    Deleting...
                  </>
                ) : (
                  'Delete'
                )}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default ModelInspectorPanel;
