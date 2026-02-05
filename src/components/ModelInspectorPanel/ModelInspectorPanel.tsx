import { FC, useCallback, useState, useEffect } from 'react';
import { appLogger } from '../../services/platform';
import { GgufModel, ServerInfo, HfModelSummary } from '../../types';
import { queueDownload } from '../../services/clients/downloads';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import { useSettings } from '../../hooks/useSettings';
import { useToastContext } from '../../contexts/ToastContext';
import { HfModelPreview } from '../HfModelPreview';
import { LlamaInstallModal } from '../LlamaInstallModal';
import { LlamaServerNotInstalledMetadata } from '../../services/transport/errors';
import {
  useEditMode,
  useModelTags,
  useServeModal,
  useDeleteModal,
  useServerActions,
} from './hooks';
import {
  ModelMetadataGrid,
  ModelEditForm,
  TagChips,
  TagAddInput,
  ServeModal,
  DeleteModal,
  InspectorActions,
} from './components';
import { Input } from '../ui/Input';
import './ModelInspectorPanel.css';

interface ModelInspectorPanelProps {
  model: GgufModel | null;
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
  onRefresh?: () => Promise<void>;
  queueStatus?: DownloadQueueStatus | null;
  onRegisterServeModalOpener?: (opener: () => void) => void;
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
  onRegisterServeModalOpener,
}) => {
  const { settings } = useSettings();
  const { showToast } = useToastContext();
  
  // State for llama-server install modal
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [installMetadata, setInstallMetadata] = useState<LlamaServerNotInstalledMetadata | null>(null);
  
  // Callback for when llama-server is not installed
  const handleLlamaServerNotInstalled = useCallback((metadata: LlamaServerNotInstalledMetadata) => {
    setInstallMetadata(metadata);
    setShowInstallModal(true);
  }, []);

  // Hooks for state management
  const editMode = useEditMode(model);
  const tags = useModelTags({
    modelId: model?.id,
    getModelTags,
    onAddTag,
    onRemoveTag,
    onRefresh,
  });
  const serveModal = useServeModal(model?.id);
  const deleteModal = useDeleteModal();

  // Register serve modal opener for menu actions
  useEffect(() => {
    if (onRegisterServeModalOpener && model) {
      onRegisterServeModalOpener(serveModal.openServeModal);
    }
  }, [onRegisterServeModalOpener, model, serveModal.openServeModal]);

  // Compute derived state
  const combinedTags = tags.modelTags.length > 0 ? tags.modelTags : (model?.tags || []);
  const hasAgentTag = combinedTags.some(tag => tag.toLowerCase() === 'agent');

  // Server actions hook
  const serverActions = useServerActions({
    model,
    settings,
    servers,
    editedName: editMode.editedName,
    editedQuantization: editMode.editedQuantization,
    editedFilePath: editMode.editedFilePath,
    editedInferenceDefaults: editMode.editedInferenceDefaults,
    customContext: serveModal.customContext,
    customPort: serveModal.customPort,
    jinjaOverride: serveModal.jinjaOverride,
    hasAgentTag,
    inferenceParams: serveModal.inferenceParams,
    onStopServer,
    onRemoveModel,
    onUpdateModel,
    onStartServer,
    onServerStarted,
    onLlamaServerNotInstalled: handleLlamaServerNotInstalled,
    setIsServing: serveModal.setIsServing,
    setIsDeleting: deleteModal.setIsDeleting,
    closeServeModal: serveModal.closeServeModal,
    closeDeleteModal: deleteModal.closeDeleteModal,
    resetEditState: editMode.resetEditState,
  });

  // Download handler for HF models
  const handleHfDownload = useCallback(async (modelId: string, quantization: string) => {
    try {
      await queueDownload({ modelId, quantization });
      showToast(`Download queued: ${modelId}`, 'success');
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to start download';
      showToast(message, 'error');
      appLogger.error('component.model', 'Failed to start download', { error });
    }
  }, [showToast]);

  // Queue status for download button
  const maxQueueSize = queueStatus?.max_size ?? 3;
  const currentQueueCount = (queueStatus?.current ? 1 : 0) + (queueStatus?.pending?.length ?? 0);
  const downloadsDisabled = currentQueueCount >= maxQueueSize;
  const disabledReason = downloadsDisabled 
    ? `Download queue is full (${currentQueueCount}/${maxQueueSize})`
    : undefined;

  // Handle toggle server (open modal or stop)
  const handleToggleServer = useCallback(() => {
    if (serverActions.isRunning) {
      serverActions.handleToggleServer();
    } else {
      serveModal.openServeModal();
    }
  }, [serverActions, serveModal]);

  // If HuggingFace model is selected, show HF model preview
  if (selectedHfModel) {
    return (
      <div className="mcc-panel inspector-panel hf-preview-panel">
        <HfModelPreview
          model={selectedHfModel}
          onDownload={handleHfDownload}
          downloadsDisabled={downloadsDisabled}
          disabledReason={disabledReason}
        />
      </div>
    );
  }

  // Empty state
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
        {editMode.isEditMode ? (
          <Input
            type="text"
            className="inspector-title-edit"
            value={editMode.editedName}
            onChange={(e) => editMode.setEditedName(e.target.value)}
            placeholder="Model name"
          />
        ) : (
          <h2 className="inspector-title">{model.name}</h2>
        )}
      </div>

      <div className="mcc-panel-content">
        <div className="inspector-content">
          {/* Metadata Section */}
          {editMode.isEditMode ? (
            <ModelEditForm
              model={model}
              editedQuantization={editMode.editedQuantization}
              editedFilePath={editMode.editedFilePath}
              editedInferenceDefaults={editMode.editedInferenceDefaults}
              onQuantizationChange={editMode.setEditedQuantization}
              onFilePathChange={editMode.setEditedFilePath}
              onInferenceDefaultsChange={editMode.setEditedInferenceDefaults}
            />
          ) : (
            <ModelMetadataGrid model={model} />
          )}

          {/* Tags Section */}
          <section className="inspector-section">
            <h3>Tags</h3>
            <div className="tags-container">
              <TagChips 
                tags={tags.modelTags} 
                onRemoveTag={tags.handleRemoveTag} 
              />
              <TagAddInput
                value={tags.newTag}
                onChange={tags.setNewTag}
                onAdd={tags.handleAddTag}
              />
            </div>
          </section>

          {/* Actions Section */}
          <InspectorActions
            isRunning={serverActions.isRunning}
            isEditMode={editMode.isEditMode}
            onToggleServer={handleToggleServer}
            onEdit={editMode.handleEdit}
            onSave={serverActions.handleSave}
            onCancel={editMode.handleCancel}
            onDelete={deleteModal.openDeleteModal}
          />
        </div>
      </div>

      {/* Serve Modal */}
      {serveModal.showServeModal && (
        <ServeModal
          model={model}
          settings={settings}
          customContext={serveModal.customContext}
          customPort={serveModal.customPort}
          jinjaOverride={serveModal.jinjaOverride}
          isServing={serveModal.isServing}
          hasAgentTag={hasAgentTag}
          inferenceParams={serveModal.inferenceParams}
          onContextChange={serveModal.setCustomContext}
          onPortChange={serveModal.setCustomPort}
          onJinjaChange={serveModal.setJinjaOverride}
          onJinjaReset={() => serveModal.setJinjaOverride(null)}
          onInferenceParamsChange={serveModal.setInferenceParams}
          onClose={serveModal.closeServeModal}
          onStart={serverActions.handleStartServer}
        />
      )}

      {/* Delete Modal */}
      {deleteModal.showDeleteModal && (
        <DeleteModal
          model={model}
          isDeleting={deleteModal.isDeleting}
          onClose={deleteModal.closeDeleteModal}
          onConfirm={serverActions.handleConfirmDelete}
        />
      )}
      
      {/* Llama Server Install Modal */}
      {showInstallModal && installMetadata && (
        <LlamaInstallModal
          isOpen={showInstallModal}
          onClose={() => setShowInstallModal(false)}
          metadata={installMetadata}
        />
      )}
    </div>
  );
};

export default ModelInspectorPanel;
