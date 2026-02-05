import { useState, useRef, useCallback, lazy, Suspense } from 'react';
import { useModels } from '../hooks/useModels';
import { useTags } from '../hooks/useTags';
import { useDownloadManager } from '../hooks/useDownloadManager';
import { useDownloadCompletionEffects } from '../hooks/useDownloadCompletionEffects';
import { useModelFilterOptions } from '../hooks/useModelFilterOptions';
import { useToastContext } from '../contexts/ToastContext';
import { useDownloadSystemStatus } from '../hooks/useDownloadSystemStatus';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import { ModelInspectorPanel } from '../components/ModelInspectorPanel';
import { GlobalDownloadStatus } from '../components/GlobalDownloadStatus';
import { useMccFilters } from './modelControlCenter/useMccFilters';
import { useMccLayout } from './modelControlCenter/useMccLayout';
import { useMccMenuActions } from './modelControlCenter/useMccMenuActions';
// Lazy load ChatPage to avoid loading assistant-ui until needed
const ChatPage = lazy(() => import('./ChatPage'));
import { ServerInfo, HfModelSummary } from '../types';
import { SidebarTabId } from '../components/ModelLibraryPanel/SidebarTabs';
import { AddDownloadSubTab } from '../components/ModelLibraryPanel/AddDownloadContent';
import './ModelControlCenterPage.css';

interface ChatSession {
  serverPort: number;
  modelId: number;
  modelName: string;
  initialView: 'chat' | 'console';
}

interface ModelControlCenterPageProps {
  servers: ServerInfo[];
  loadServers: () => Promise<void>;
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
  loadServers,
  stopServer,
  onRegisterMenuActions,
}: ModelControlCenterPageProps) {
  const { models, selectedModel, selectedModelId, loading, error, loadModels, selectModel, addModel, removeModel, updateModel } = useModels();
  const { tags, loadTags, addTagToModel, removeTagFromModel, getModelTags } = useTags();
  const { showToast } = useToastContext();
  const { filterOptions, refresh: refreshFilterOptions } = useModelFilterOptions();
  
  // Unified refresh function for models, filter options, and tags
  const handleRefreshAll = useCallback(async () => {
    await Promise.all([loadModels(), refreshFilterOptions(), loadTags()]);
  }, [loadModels, refreshFilterOptions, loadTags]);
  
  // Download completion effects - batches completions, triggers refresh, shows toast
  const { onCompleted } = useDownloadCompletionEffects({
    refreshModels: handleRefreshAll,
  });
  
  // Global download progress - lifted to page level so it's always visible
  const {
    currentProgress,
    queueStatus,
    downloadUiState,
    lastQueueSummary,
    cancel: cancelDownload,
    refreshQueue,
    clearQueueSummary,
  } = useDownloadManager({
    onCompleted,
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
  
  // Chat session state - when set, shows ChatPage instead of model panels
  const [chatSession, setChatSession] = useState<ChatSession | null>(null);
  
  // Ref for file input (for menu-triggered file add)
  const fileInputRef = useRef<HTMLInputElement>(null);
  
  // Ref for opening serve modal from menu
  const openServeModalRef = useRef<(() => void) | null>(null);
  
  // Panel width state (percentages) - now just two columns
  const { leftPanelWidth, layoutRef, handleMouseDown } = useMccLayout();

  const openChatSession = useCallback(
    (modelId: number, view: 'chat' | 'console') => {
      const server = servers.find((s) => s.modelId === modelId);
      if (server) {
        setChatSession({
          serverPort: server.port,
          modelId: server.modelId,
          modelName: server.modelName,
          initialView: view,
        });
      }
    },
    [servers]
  );

  useMccMenuActions({
    onRegisterMenuActions,
    selectedModelId,
    servers,
    models,
    loadServers,
    stopServer,
    removeModel,
    selectModel,
    setSidebarTab,
    setActiveSubTab: (tab: AddDownloadSubTab) => setActiveSubTab(tab),
    triggerFilePicker: () => fileInputRef.current?.click(),
    refreshAll: handleRefreshAll,
    chatSessionModelId: chatSession?.modelId ?? null,
    closeChatSession: () => setChatSession(null),
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
    selectModel(id);
    if (id !== null) {
      setSelectedHfModel(null); // Clear HF selection when selecting local model
    }
  };

  // Handler for selecting an HF model for preview (clears local selection)
  const handleSelectHfModel = (model: HfModelSummary | null) => {
    setSelectedHfModel(model);
    if (model !== null) {
      selectModel(null); // Clear local model selection when selecting HF model
    }
  };

  // Handler for sidebar tab changes - manages model selection based on context
  const handleSidebarTabChange = (tab: SidebarTabId) => {
    setSidebarTab(tab);
    // Clear appropriate model selection based on tab context
    if (tab === 'add') {
      // Clear local model selection when entering Add Models tab
      selectModel(null);
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
  const handleServerStarted = async (serverInfo: ServerInfo) => {
    // Server started, open chat
    setChatSession({
      serverPort: serverInfo.port,
      modelId: serverInfo.modelId,
      modelName: serverInfo.modelName,
      initialView: 'chat',
    });
  };

  // Handler to close chat and stop server
  const handleCloseChat = async () => {
    if (chatSession) {
      await stopServer(chatSession.modelId);
      setChatSession(null);
    }
  };

  // If chat session is active, show ChatPage
  if (chatSession) {
    return (
      <Suspense fallback={<div className="model-control-center"><div className="loading-chat">Loading chat...</div></div>}>
        <ChatPage
          serverPort={chatSession.serverPort}
          modelId={chatSession.modelId}
          modelName={chatSession.modelName}
          initialView={chatSession.initialView}
          onClose={handleCloseChat}
        />
      </Suspense>
    );
  }

  return (
    <div className="model-control-center">
      <div 
        ref={layoutRef}
        className="mcc-layout"
        style={{
          gridTemplateColumns: `${leftPanelWidth}% ${100 - leftPanelWidth}%`
        }}
      >
        {/* Left Panel: Model Library */}
        <div className="grid-panel-container">
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
          />
          <div 
            className="resize-handle" 
            onMouseDown={handleMouseDown}
          />
        </div>

        {/* Right Panel: Model Inspector */}
        <div className="grid-panel-container right-panel-container">
          {/* Global Download Status - always visible regardless of selected tab/model */}
          {!downloadDismissed && (
            <GlobalDownloadStatus
              progress={currentProgress}
              queueStatus={queueStatus}
              downloadUiState={downloadUiState}
              lastQueueSummary={lastQueueSummary}
              onCancel={cancelDownload}
              onDismissSummary={clearQueueSummary}
              onRefreshQueue={refreshQueue}
            />
          )}
          
          <ModelInspectorPanel
            model={selectedModel}
            selectedHfModel={selectedHfModel}
            onStartServer={loadServers}
            onServerStarted={handleServerStarted}
            onStopServer={stopServer}
            servers={servers}
            onRemoveModel={removeModel}
            onUpdateModel={updateModel}
            onAddTag={addTagToModel}
            onRemoveTag={removeTagFromModel}
            getModelTags={getModelTags}
            onRefresh={handleRefreshAll}
            queueStatus={queueStatus}
            onRegisterServeModalOpener={(opener) => { openServeModalRef.current = opener; }}
          />
        </div>
      </div>
    </div>
  );
}
