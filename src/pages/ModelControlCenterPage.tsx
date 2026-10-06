import { useState, useRef, useCallback, lazy, Suspense } from 'react';
import { useModels } from '../hooks/useModels';
import { usePairedModels } from '../hooks/usePairedModels';
import { useLibrarySelection } from '../hooks/useLibrarySelection';
import { useTags } from '../hooks/useTags';
import { useDownloadManager } from '../hooks/useDownloadManager';
import { useDownloadCompletionEffects } from '../hooks/useDownloadCompletionEffects';
import { useModelLibraryEvents } from '../hooks/useModelLibraryEvents';
import { useModelFilterOptions } from '../hooks/useModelFilterOptions';
import { useToastContext } from '../contexts/ToastContext';
import { useDownloadSystemStatus } from '../hooks/useDownloadSystemStatus';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import { FarModelInspector, ModelInspectorPanel } from '../components/ModelInspectorPanel';
import { GlobalDownloadStatus } from '../components/GlobalDownloadStatus';
import TwoPanelLayout from '../components/TwoPanelLayout';
import { useMccFilters } from './modelControlCenter/useMccFilters';
import { useMccLayout } from './modelControlCenter/useMccLayout';
import { useMccMenuActions } from './modelControlCenter/useMccMenuActions';
import { useChatSession } from './modelControlCenter/useChatSession';
// Lazy load ChatPage to avoid loading assistant-ui until needed
const ChatPage = lazy(() => import('./ChatPage'));
// Lazy load BenchmarkPage to keep initial bundle small
const BenchmarkPage = lazy(() => import('./BenchmarkPage'));
import { HfModelSummary } from '../types';
import type { ServerViewModel } from '../hooks/useServers';
import { SidebarTabId } from '../components/ModelLibraryPanel/ModelLibraryPanel';
import { AddDownloadSubTab } from '../components/ModelLibraryPanel/AddDownloadContent';
import { getTransport } from '../services/transport';

interface ModelControlCenterPageProps {
  servers: ServerViewModel[];
  stopServer: (modelId: number) => Promise<void>;
  onRegisterMenuActions?: (actions: {
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    showChat: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  }) => void;
}

export default function ModelControlCenterPage({
  servers,
  stopServer,
  onRegisterMenuActions,
}: ModelControlCenterPageProps) {
  const { models, selectedModel, selectedModelId, loading, error, loadModels, selectModel, addModel, removeModel, updateModel } = useModels();
  // The paired machine's rows, and one selection across both machines.
  const paired = usePairedModels();
  const { farPick, pickFar, pickLocal } = useLibrarySelection(selectModel, paired.group);
  const { tags, loadTags, addTagToModel, removeTagFromModel } = useTags();
  const { showToast } = useToastContext();
  const { filterOptions, refresh: refreshFilterOptions } = useModelFilterOptions();
  
  // Unified refresh function for models, filter options, and tags
  const handleRefreshAll = useCallback(async () => {
    await Promise.all([loadModels(), refreshFilterOptions(), loadTags()]);
  }, [loadModels, refreshFilterOptions, loadTags]);
  
  // Another client's edits. Mounted here rather than inside `useModels`
  // because a library change moves more than the list: a new architecture or
  // tag has to reach the filter options too, or the user can see the model
  // that appeared but cannot filter to it.
  useModelLibraryEvents(handleRefreshAll);

  // Download ending effects - batches completions, triggers refresh, shows toasts
  const { onCompleted, onFailed } = useDownloadCompletionEffects({
    refreshModels: handleRefreshAll,
  });
  
  // The download queue - lifted to page level so it's always visible
  const {
    snapshot: downloadQueue,
    cancellingId,
    lastQueueSummary,
    cancel: cancelDownload,
    refreshQueue,
    clearQueueSummary,
  } = useDownloadManager({
    onCompleted,
    onFailed,
  });

  // Backend download system initialization (Python fast helper)
  const downloadSystem = useDownloadSystemStatus();
  const downloadSystemError = downloadSystem.status === 'error' ? downloadSystem.message : null;
  
  // Track whether user dismissed completion banner
  const [downloadDismissed] = useState(false);
  
  // Sidebar tab state (for the new tabbed sidebar)
  const [sidebarTab, setSidebarTab] = useState<SidebarTabId>('models');
  
  // HuggingFace model selection state (for preview in inspector)
  const [selectedHfModel, setSelectedHfModel] = useState<HfModelSummary | null>(null);
  
  // Chat session state - when set, shows ChatPage instead of model panels.
  // Local sessions come from a served model here, paired ones from a far row.
  const { chatSession, setChatSession, openChatSession, openPairedChat, switchChatModel, startingModel, closeChatSession } =
    useChatSession(servers);

  // Benchmark state - when set, shows BenchmarkPage instead of model panels
  const [benchmarkModelId, setBenchmarkModelId] = useState<number | null>(null);
  
  // Ref for file input (for menu-triggered file add)
  const fileInputRef = useRef<HTMLInputElement>(null);
  
  // Ref for opening serve modal from menu
  const openServeModalRef = useRef<(() => void) | null>(null);
  
  // Panel width state (percentages) - now just two columns
  const { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize } = useMccLayout();

  useMccMenuActions({
    onRegisterMenuActions,
    selectedModelId,
    servers,
    models,
    stopServer,
    removeModel,
    selectModel: pickLocal,
    setSidebarTab,
    setActiveSubTab: (tab: AddDownloadSubTab) => setActiveSubTab(tab),
    triggerFilePicker: () => fileInputRef.current?.click(),
    refreshAll: handleRefreshAll,
    // Only a local session can be the one whose server just stopped.
    chatSessionModelId: chatSession?.kind === 'local' ? chatSession.modelId : null,
    closeChatSession,
    openChatSession,
    onOpenServeModal: () => openServeModalRef.current?.(),
    showToast,
  });

  const {
    searchQuery,
    setSearchQuery,
    filters,
    onFiltersChange: handleFiltersChange,
    onClearFilters: handleClearFilters,
    filteredModels,
    activeSubTab,
    setActiveSubTab,
    handleModelAdded,
  } = useMccFilters({
    models,
    addModel,
    loadModels,
    refreshFilterOptions,
    loadTags,
  });

  // Handler for selecting a local model (clears HF selection)
  const handleSelectLocalModel = (id: number | null) => {
    pickLocal(id);
    if (id !== null) {
      setSelectedHfModel(null); // Clear HF selection when selecting local model
    }
  };

  // Handler for selecting an HF model for preview (clears local selection)
  const handleSelectHfModel = (model: HfModelSummary | null) => {
    setSelectedHfModel(model);
    if (model !== null) {
      pickLocal(null); // Clear the library's selection when selecting HF model
    }
  };

  // Handler for sidebar tab changes - manages model selection based on context
  const handleSidebarTabChange = (tab: SidebarTabId) => {
    setSidebarTab(tab);
    // Clear appropriate model selection based on tab context
    if (tab === 'add') {
      // Clear the library's selection when entering Add Models tab
      pickLocal(null);
    } else {
      // Clear HF model selection when leaving the Add Models tab
      setSelectedHfModel(null);
    }
  };

  // Handler for subtab changes within Add Models - clears HF selection when leaving Browse HF
  const handleSubTabChange = (subtab: AddDownloadSubTab) => {
    setActiveSubTab(subtab);
    // Clear HF model selection when switching away from Browse HF subtab
    if (subtab !== 'browse') {
      setSelectedHfModel(null);
    }
  };

  // Handler for when server starts - opens chat view
  const handleServerStarted = async (serverInfo: ServerViewModel) => {
    setChatSession({
      kind: 'local',
      serverPort: serverInfo.port,
      modelId: serverInfo.modelId,
      modelName: serverInfo.modelName,
      initialView: 'chat',
    });
  };

  // If chat session is active, show ChatPage. Keyed by the server, so a
  // model switch remounts it on the new one. Close only leaves the chat: the
  // proxy serves its model to every client, Copilot included, so unloading
  // it is a separate action, and only a model served here can be unloaded.
  if (chatSession) {
    return (
      <Suspense fallback={<div className="flex flex-col h-full w-full overflow-hidden"><div className="loading-chat">Loading chat...</div></div>}>
        {chatSession.kind === 'local' ? (
          <ChatPage
            key={`${chatSession.modelId}:${chatSession.serverPort}`}
            serverPort={chatSession.serverPort}
            modelId={chatSession.modelId}
            modelName={chatSession.modelName}
            initialView={chatSession.initialView}
            conversationId={chatSession.conversationId}
            draft={chatSession.draft}
            startingModel={startingModel}
            onSwitchModel={(choice, context) => switchChatModel(chatSession, choice, context)}
            onUnloadModel={() => stopServer(chatSession.modelId)}
            onClose={closeChatSession}
          />
        ) : (
          <ChatPage paired={chatSession} modelName={chatSession.modelName} onClose={closeChatSession} />
        )}
      </Suspense>
    );
  }

  // If benchmark is active, show BenchmarkPage
  if (benchmarkModelId !== null) {
    return (
      <Suspense fallback={<div className="flex flex-col h-full w-full overflow-hidden items-center justify-center text-text-muted">Loading benchmark...</div>}>
        <BenchmarkPage
          models={models}
          initialModelIds={[benchmarkModelId]}
          onClose={() => setBenchmarkModelId(null)}
        />
      </Suspense>
    );
  }

  return (
    <div className="flex flex-col w-full min-h-full overflow-auto md:h-full md:min-h-0 md:overflow-hidden">
      <TwoPanelLayout
        ref={layoutRef}
        className="overflow-visible"
        leftWidth={leftPanelWidth}
        onResizeStart={handlePointerDown}
        onKeyboardResize={handleKeyboardResize}
        left={
          <ModelLibraryPanel
            models={filteredModels}
            selectedModelId={selectedModelId}
            onSelectModel={handleSelectLocalModel}
            loading={loading}
            error={error}
            onRefresh={loadModels}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            tags={tags}
            servers={servers}
            filterOptions={filterOptions}
            filters={filters}
            onFiltersChange={handleFiltersChange}
            onClearFilters={handleClearFilters}
            onModelAdded={handleModelAdded}
            activeSubTab={activeSubTab}
            onSubTabChange={handleSubTabChange}
            downloadSystemError={downloadSystemError}
            onSelectHfModel={handleSelectHfModel}
            selectedHfModelId={selectedHfModel?.id}
            activeTab={sidebarTab}
            onTabChange={handleSidebarTabChange}
            paired={paired}
            farPick={farPick}
            onPickFar={(model) => { pickFar(model); setSelectedHfModel(null); }}
          />
        }
        rightClassName="gap-0"
        right={
          <>
            {!downloadDismissed && (
              <GlobalDownloadStatus
                snapshot={downloadQueue}
                cancellingId={cancellingId}
                lastQueueSummary={lastQueueSummary}
                onCancel={cancelDownload}
                onDismissSummary={clearQueueSummary}
                onRefreshQueue={refreshQueue}
              />
            )}
            {farPick ? (
              <FarModelInspector model={farPick} paired={paired} onChat={(far, name) => openPairedChat(far, name, paired.name)} />
            ) : (
              <ModelInspectorPanel
                model={selectedModel}
                selectedHfModel={selectedHfModel}
                onServerStarted={handleServerStarted}
                onOpenChat={(modelId) => openChatSession(modelId, 'chat')}
                onStopServer={stopServer}
                servers={servers}
                onRemoveModel={removeModel}
                onUpdateModel={updateModel}
                onAddTag={addTagToModel}
                onRemoveTag={removeTagFromModel}
                getModelDetail={(id) => getTransport().getModelDetail(id)}
                onRefresh={handleRefreshAll}
                downloadQueue={downloadQueue}
                onRegisterServeModalOpener={(opener) => { openServeModalRef.current = opener; }}
                onBenchmark={(modelId) => setBenchmarkModelId(modelId)}
              />
            )}
          </>
        }
      />
    </div>
  );
}
