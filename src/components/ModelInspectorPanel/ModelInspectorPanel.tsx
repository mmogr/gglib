import { FC, useCallback, useEffect } from 'react';
import { cn } from '../../utils/cn';
import { appLogger } from '../../services/platform';
import { GgufModel, ModelDetail, ServerInfo, HfModelSummary } from '../../types';
import { queueDownload } from '../../services/clients/downloads';
import type { DownloadQueueStatus } from '../../services/transport/types/downloads';
import { useSettings } from '../../hooks/useSettings';
import { useToastContext } from '../../contexts/ToastContext';
import { HfModelPreview } from '../HfModelPreview';
import {
  useEditMode,
  useModelDetail,
  useServeModal,
  useDeleteModal,
  useServerActions,
  useInspectorModals,
} from './hooks';
import {
  ModelMetadataGrid,
  ModelEditForm,
  InspectorTags,
  ServeModal,
  DeleteModal,
  InspectorHeader,
  InspectorFooter,
  InspectorEmptyState,
  InspectorModals,
} from './components';

/**
 * Outer shell. `overflow-hidden` (not `auto`) so the header and footer stay
 * pinned and only the middle section scrolls.
 */
const panelContainer = "flex flex-col overflow-hidden relative flex-1 bg-surface md:h-full md:min-h-0";

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
    serverDefaults?: import('../../types').ServerConfig | null;
  }) => Promise<void>;
  onAddTag: (modelId: number, tag: string) => Promise<void>;
  onRemoveTag: (modelId: number, tag: string) => Promise<void>;
  getModelDetail: (modelId: number) => Promise<ModelDetail | null>;
  onRefresh?: () => Promise<void>;
  queueStatus?: DownloadQueueStatus | null;
  onRegisterServeModalOpener?: (opener: () => void) => void;
  onBenchmark?: (modelId: number) => void;
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
  getModelDetail,
  onRefresh,
  queueStatus,
  onRegisterServeModalOpener,
  onBenchmark,
}) => {
  const { settings } = useSettings();
  const { showToast } = useToastContext();

  // Install / verify / update modal state
  const modals = useInspectorModals();

  // Hooks for state management
  const editMode = useEditMode(model);
  const detail = useModelDetail({
    modelId: model?.id,
    getModelDetail,
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

  // Compute derived state — tags are authoritative from the detail response
  const hasAgentTag = detail.tags.some(tag => tag.toLowerCase() === 'agent');
  const hasMtpTag = detail.tags.some(tag => tag.toLowerCase() === 'mtp');

  // Server actions hook
  const serverActions = useServerActions({
    model,
    settings,
    servers,
    editedName: editMode.editedName,
    editedQuantization: editMode.editedQuantization,
    editedFilePath: editMode.editedFilePath,
    editedInferenceDefaults: editMode.editedInferenceDefaults,
    editedServerDefaults: editMode.editedServerDefaults,
    customContext: serveModal.customContext,
    customPort: serveModal.customPort,
    jinjaOverride: serveModal.jinjaOverride,
    hasAgentTag,
    hasMtpTag,
    mtpNMaxOverride: serveModal.mtpNMaxOverride,
    mtpPMinOverride: serveModal.mtpPMinOverride,
    inferenceParams: serveModal.inferenceParams,
    onStopServer,
    onRemoveModel,
    onUpdateModel,
    onStartServer,
    onServerStarted,
    onLlamaServerNotInstalled: modals.handleLlamaServerNotInstalled,
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
      <div className={cn(panelContainer, "overflow-hidden")}>
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
      <div className={panelContainer}>
        <InspectorEmptyState />
      </div>
    );
  }

  return (
    <div className={panelContainer}>
      <InspectorHeader
        modelName={model.name}
        hasHfRepo={Boolean(model.hfRepoId)}
        isEditMode={editMode.isEditMode}
        editedName={editMode.editedName}
        onEditedNameChange={editMode.setEditedName}
        onVerify={modals.openVerifyModal}
        onCheckUpdates={modals.openUpdateModal}
      />

      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        <div className="p-base">
          {/* Metadata Section */}
          {editMode.isEditMode ? (
            <ModelEditForm
              model={model}
              editedQuantization={editMode.editedQuantization}
              editedFilePath={editMode.editedFilePath}
              editedInferenceDefaults={editMode.editedInferenceDefaults}
              editedServerDefaults={editMode.editedServerDefaults}
              onQuantizationChange={editMode.setEditedQuantization}
              onFilePathChange={editMode.setEditedFilePath}
              onInferenceDefaultsChange={editMode.setEditedInferenceDefaults}
              onServerDefaultsChange={editMode.setEditedServerDefaults}
            />
          ) : (
            <ModelMetadataGrid model={model} detail={detail.modelDetail ?? undefined} />
          )}

          <InspectorTags
            tags={detail.tags}
            newTagInput={detail.newTagInput}
            onNewTagInputChange={detail.setNewTagInput}
            onAddTag={detail.addTag}
            onRemoveTag={detail.removeTag}
          />
        </div>
      </div>

      <InspectorFooter
        isRunning={serverActions.isRunning}
        isEditMode={editMode.isEditMode}
        onToggleServer={handleToggleServer}
        onEdit={editMode.handleEdit}
        onSave={serverActions.handleSave}
        onCancel={editMode.handleCancel}
        onDelete={deleteModal.openDeleteModal}
        onBenchmark={onBenchmark && model?.id != null ? () => onBenchmark(model.id!) : undefined}
      />

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
          hasMtpTag={hasMtpTag}
          mtpNMaxOverride={serveModal.mtpNMaxOverride}
          mtpPMinOverride={serveModal.mtpPMinOverride}
          inferenceParams={serveModal.inferenceParams}
          onContextChange={serveModal.setCustomContext}
          onPortChange={serveModal.setCustomPort}
          onJinjaChange={serveModal.setJinjaOverride}
          onJinjaReset={() => serveModal.setJinjaOverride(null)}
          onMtpNMaxChange={serveModal.setMtpNMaxOverride}
          onMtpPMinChange={serveModal.setMtpPMinOverride}
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

      <InspectorModals model={model} modals={modals} />
    </div>
  );
};

export default ModelInspectorPanel;
