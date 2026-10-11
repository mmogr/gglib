import { FC, useCallback, useEffect } from 'react';
import { cn } from '../../utils/cn';
import { GgufModel, ModelDetail, HfModelSummary } from '../../types';
import type { ServerViewModel } from '../../hooks/useServers';
import type { QueueSnapshot } from '../../services/transport/types/downloads';
import { useSettings } from '../../hooks/useSettings';
import { useToastContext } from '../../contexts/ToastContext';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { HfModelPreview } from '../HfModelPreview';
import {
  useEditMode,
  useModelDetail,
  useServeModal,
  useDeleteModal,
  useServerActions,
  useRetagModel,
  useInspectorModals,
  useHfDownload,
} from './hooks';
import {
  ModelMetadataGrid,
  ProjectorRow,
  ComponentsRow,
  SamplingProvenanceSection,
  ModelEditForm,
  InspectorTags,
  InspectorCapabilities,
  ReasoningSupport,
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
  onServerStarted?: (serverInfo: ServerViewModel) => void;
  /** Open the chat screen on this model's running server. */
  onOpenChat?: (modelId: number) => void;
  onStopServer: (modelId: number) => Promise<void>;
  servers: ServerViewModel[];
  onRemoveModel: (id: number, force: boolean) => void;
  onUpdateModel: (id: number, updates: import('../../hooks/useModels').ModelUpdates) => Promise<void>;
  onAddTag: (modelId: number, tag: string) => Promise<void>;
  onRemoveTag: (modelId: number, tag: string) => Promise<void>;
  getModelDetail: (modelId: number) => Promise<ModelDetail | null>;
  onRefresh?: () => Promise<void>;
  /** The download queue, which says whether it has room for another */
  downloadQueue?: QueueSnapshot | null;
  onRegisterServeModalOpener?: (opener: () => void) => void;
  onBenchmark?: (modelId: number) => void;
  /** Open Settings on its System tab, where the image runtime is installed. */
  onOpenSystemSettings?: () => void;
}

const ModelInspectorPanel: FC<ModelInspectorPanelProps> = ({
  model,
  selectedHfModel,
  onServerStarted,
  onOpenChat,
  onStopServer,
  servers,
  onRemoveModel,
  onUpdateModel,
  onAddTag,
  onRemoveTag,
  getModelDetail,
  onRefresh,
  downloadQueue,
  onRegisterServeModalOpener,
  onBenchmark,
  onOpenSystemSettings,
}) => {
  const { settings } = useSettings();
  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();

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

  const { retagging, handleRetag } = useRetagModel({
    modelId: model?.id,
    reload: detail.reload,
    showToast,
    confirm,
  });

  const serverActions = useServerActions({
    model,
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
    mtpNMaxOverride: serveModal.mtpNMaxOverride,
    mtpPMinOverride: serveModal.mtpPMinOverride,
    inferenceParams: serveModal.inferenceParams,
    pinProxy: serveModal.pinProxy,
    onStopServer,
    onRemoveModel,
    onUpdateModel,
    onServerStarted,
    onLlamaServerNotInstalled: modals.handleLlamaServerNotInstalled,
    setIsServing: serveModal.setIsServing,
    setIsDeleting: deleteModal.setIsDeleting,
    closeServeModal: serveModal.closeServeModal,
    closeDeleteModal: deleteModal.closeDeleteModal,
    resetEditState: editMode.resetEditState,
  });

  // Download handler and queue room for HF models
  const { handleHfDownload, downloadsDisabled, disabledReason } = useHfDownload(downloadQueue);

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
        model={model}
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
              reasoningEffortSupport={detail.modelDetail?.reasoningEffortSupport}
              onQuantizationChange={editMode.setEditedQuantization}
              onFilePathChange={editMode.setEditedFilePath}
              onInferenceDefaultsChange={editMode.setEditedInferenceDefaults}
              onServerDefaultsChange={editMode.setEditedServerDefaults}
            />
          ) : (
            <ModelMetadataGrid
              model={model}
              detail={detail.modelDetail ?? undefined}
              projector={
                <ProjectorRow
                  modelId={model.id}
                  detail={detail.modelDetail ?? undefined}
                  onUpdateModel={onUpdateModel}
                  onChanged={() => void detail.reload()}
                />
              }
              components={
                model.imageFamily && (
                  <ComponentsRow
                    modelId={model.id}
                    detail={detail.modelDetail ?? undefined}
                    onUpdateModel={onUpdateModel}
                    onChanged={() => void detail.reload()}
                  />
                )
              }
              // Resolved sampling, not the stored defaults: a stored value
              // that wins shows as `per-model defaults (user-set)`, and one
              // that loses is finally visible as having lost.
              sampling={
                <SamplingProvenanceSection
                  modelId={model.id}
                  profiles={settings?.inferenceProfiles ?? []}
                  refreshKey={model.inferenceDefaults}
                />
              }
            />
          )}

          <InspectorTags
            tags={detail.tags}
            newTagInput={detail.newTagInput}
            onNewTagInputChange={detail.setNewTagInput}
            onAddTag={detail.addTag}
            onRemoveTag={detail.removeTag}
            onRetag={handleRetag}
            retagging={retagging}
          />

          {!editMode.isEditMode && model?.id != null && (
            <>
              <InspectorCapabilities
                modelId={model.id}
                capabilities={detail.modelDetail?.capabilities}
                onChanged={() => void detail.reload()}
                onError={(message: string) => showToast(message, 'error')}
              />
              {/*
                Kept out of `InspectorCapabilities` deliberately: those four are
                gglib's own editable flags, and this one is an observation of
                somebody else's template that no operator may overwrite.
              */}
              <ReasoningSupport
                support={detail.modelDetail?.reasoningEffortSupport}
                isRunning={serverActions.isRunning}
                onRecheck={() => void detail.reload()}
                onStart={serveModal.openServeModal}
                isRechecking={detail.isLoading}
              />
            </>
          )}
        </div>
      </div>

      <InspectorFooter
        isRunning={serverActions.isRunning}
        isEditMode={editMode.isEditMode}
        onToggleServer={handleToggleServer}
        onOpenChat={onOpenChat && model?.id != null ? () => onOpenChat(model.id!) : undefined}
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
          reasoningEffortSupport={detail.modelDetail?.reasoningEffortSupport}
          pinProxy={serveModal.pinProxy}
          onPinProxyChange={serveModal.setPinProxy}
          onContextChange={serveModal.setCustomContext}
          onPortChange={serveModal.setCustomPort}
          onJinjaChange={serveModal.setJinjaOverride}
          onJinjaReset={() => serveModal.setJinjaOverride(null)}
          onMtpNMaxChange={serveModal.setMtpNMaxOverride}
          onMtpPMinChange={serveModal.setMtpPMinOverride}
          onInferenceParamsChange={serveModal.setInferenceParams}
          onClose={serveModal.closeServeModal}
          onStart={serverActions.handleStartServer}
          components={detail.modelDetail?.components}
          onOpenSystemSettings={
            onOpenSystemSettings
              ? () => {
                  serveModal.closeServeModal();
                  onOpenSystemSettings();
                }
              : undefined
          }
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
