import { useState, useRef, useCallback, useEffect, lazy, Suspense } from 'react';
import { useModels } from '../hooks/useModels';
import { useTags } from '../hooks/useTags';
import { useDownloadProgress } from '../hooks/useDownloadProgress';
import { useModelFilterOptions } from '../hooks/useModelFilterOptions';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import ModelInspectorPanel from '../components/ModelInspectorPanel/ModelInspectorPanel';
import { GlobalDownloadStatus } from '../components/GlobalDownloadStatus';
import { FilterState } from '../components/FilterPopover';
// Lazy load ChatPage to avoid loading assistant-ui until needed
const ChatPage = lazy(() => import('./ChatPage'));
import { TauriService } from '../services/tauri';
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
  const { filterOptions, refresh: refreshFilterOptions } = useModelFilterOptions();
  
  // Ref to hold unified refresh - allows useDownloadProgress to call the latest version
  const refreshAllRef = useRef<() => Promise<void>>();
  
  // Global download progress - lifted to page level so it's always visible
  const { progress, queueStatus, cancelDownload, fetchQueueStatus } = useDownloadProgress({
    onCompleted: () => refreshAllRef.current?.(),
  });
  const [downloadDismissed, setDownloadDismissed] = useState(false);
  
  // Reset dismissed state when a new download starts
  useEffect(() => {
    if (progress && (progress.status === 'started' || progress.status === 'downloading' || progress.status === 'progress')) {
      setDownloadDismissed(false);
    }
  }, [progress?.status]);

  const [searchQuery, setSearchQuery] = useState('');
  const [activeSubTab, setActiveSubTab] = useState<AddDownloadSubTab>('download');
  
  // Filter state for the model library (session-only, not persisted)
  const [filters, setFilters] = useState<FilterState>({
    paramRange: null,
    contextRange: null,
    selectedQuantizations: [],
    selectedTags: [],
  });

  const handleFiltersChange = useCallback((newFilters: FilterState) => {
    setFilters(newFilters);
  }, []);

  const handleClearFilters = useCallback(() => {
    setFilters({
      paramRange: null,
      contextRange: null,
      selectedQuantizations: [],
      selectedTags: [],
    });
  }, []);

  // Unified refresh function for models, filter options, and tags
  // This ensures filter UI stays in sync with model/tag changes
  const handleRefreshAll = useCallback(async () => {
    await Promise.all([
      loadModels(),
      refreshFilterOptions(),
      loadTags(),
    ]);
  }, [loadModels, refreshFilterOptions, loadTags]);

  // Keep ref updated for useDownloadProgress callback
  refreshAllRef.current = handleRefreshAll;
  
  // Sidebar tab state (for the new tabbed sidebar)
  const [sidebarTab, setSidebarTab] = useState<SidebarTabId>('models');
  
  // HuggingFace model selection state (for preview in inspector)
  const [selectedHfModel, setSelectedHfModel] = useState<HfModelSummary | null>(null);
  
  // Chat session state - when set, shows ChatPage instead of model panels
  const [chatSession, setChatSession] = useState<ChatSession | null>(null);
  
  // Ref for file input (for menu-triggered file add)
  const fileInputRef = useRef<HTMLInputElement>(null);
  
  // Panel width state (percentages) - now just two columns
  const [leftPanelWidth, setLeftPanelWidth] = useState(45);
  
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);

  // Register menu actions for App.tsx to call
  useEffect(() => {
    if (onRegisterMenuActions) {
      onRegisterMenuActions({
        refreshModels: () => {
          handleRefreshAll();
        },
        addModelFromFile: () => {
          // Switch to add tab in sidebar
          setSidebarTab('add');
          setActiveSubTab('add');
          // Also trigger the actual file picker if available
          fileInputRef.current?.click();
        },
        showDownloads: () => {
          // Switch to add tab in sidebar with download subtab
          setSidebarTab('add');
          setActiveSubTab('download');
        },
        startServer: () => {
          if (selectedModelId) {
            // Trigger start server via the inspector panel's functionality
            // The actual server start is handled through the ModelInspectorPanel
            loadServers();
          }
        },
        stopServer: async () => {
          if (selectedModelId) {
            // Find if this model has a running server
            const runningServer = servers.find(s => s.model_id === selectedModelId);
            if (runningServer) {
              await stopServer(selectedModelId);
              // Close chat if this model's chat is open
              if (chatSession?.modelId === selectedModelId) {
                setChatSession(null);
              }
              // Sync menu state after server stop
              TauriService.syncMenuStateSilent();
            }
          }
        },
        removeModel: async () => {
          if (selectedModelId) {
            await removeModel(selectedModelId, false);
            // Sync menu state after model removal
            TauriService.syncMenuStateSilent();
          }
        },
        selectModel: (modelId: number, view?: 'chat' | 'console') => {
          // If a view is specified, open the chat/console page for that server
          if (view) {
            const server = servers.find(s => s.model_id === modelId);
            if (server) {
              setChatSession({
                serverPort: server.port,
                modelId: server.model_id,
                modelName: server.model_name,
                initialView: view,
              });
            }
          } else {
            // Just select the model in the library
            selectModel(modelId);
          }
        },
      });
    }
  }, [onRegisterMenuActions, handleRefreshAll, selectedModelId, servers, stopServer, removeModel, loadServers, selectModel, chatSession]);

  // Handle resize
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingRef.current = true;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  useEffect(() => {
    let rafId: number | null = null;
    
    const handleMouseMove = (e: MouseEvent) => {
      if (!isDraggingRef.current || !layoutRef.current) return;
      
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      
      rafId = requestAnimationFrame(() => {
        if (!layoutRef.current) return;
        
        const rect = layoutRef.current.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = (x / rect.width) * 100;
        
        // Resizing left panel (simple two-column)
        const newLeftWidth = Math.max(25, Math.min(60, percentage));
        setLeftPanelWidth(newLeftWidth);
      });
    };
    
    const handleMouseUp = () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      isDraggingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    
    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
    
    return () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, []);

  // Filter models based on search, tags, param range, context range, and quantizations
  const filteredModels = models.filter(model => {
    // Text search
    const matchesSearch = !searchQuery || 
      model.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      model.architecture?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      model.hf_repo_id?.toLowerCase().includes(searchQuery.toLowerCase());
    
    // Tag filter
    const matchesTags = filters.selectedTags.length === 0 || 
      (model.tags && filters.selectedTags.some(tag => model.tags!.includes(tag)));
    
    // Parameter count filter
    const matchesParams = filters.paramRange === null || 
      (model.param_count_b >= filters.paramRange[0] && 
       model.param_count_b <= filters.paramRange[1]);
    
    // Context length filter
    const matchesContext = filters.contextRange === null || 
      model.context_length === undefined ||
      model.context_length === null ||
      (model.context_length >= filters.contextRange[0] && 
       model.context_length <= filters.contextRange[1]);
    
    // Quantization filter
    const matchesQuantization = filters.selectedQuantizations.length === 0 || 
      (model.quantization && filters.selectedQuantizations.includes(model.quantization));
    
    return matchesSearch && matchesTags && matchesParams && matchesContext && matchesQuantization;
  });

  const handleModelAdded = async (filePath: string) => {
    if (filePath) {
      await addModel(filePath);
    }
    // Refresh all filter-related data when a model is added
    await handleRefreshAll();
  };

  const handleModelDownloaded = async () => {
    // Refresh all filter-related data when a model is downloaded
    await handleRefreshAll();
  };

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
      modelId: serverInfo.model_id,
      modelName: serverInfo.model_name,
      initialView: 'chat',
    });
  };

  // Handler to close chat and stop server
  const handleCloseChat = async () => {
    if (chatSession) {
      await stopServer(chatSession.modelId);
      setChatSession(null);
      TauriService.syncMenuStateSilent();
    }
  };

  // If chat session is active, show ChatPage
  if (chatSession) {
    return (
      <Suspense fallback={<div className="model-control-center"><div className="loading-chat">Loading chat...</div></div>}>
        <ChatPage
          serverPort={chatSession.serverPort}
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
            onModelDownloaded={handleModelDownloaded}
            activeSubTab={activeSubTab}
            onSubTabChange={handleSubTabChange}
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
              progress={progress}
              queueStatus={queueStatus}
              onCancel={cancelDownload}
              onDismiss={() => setDownloadDismissed(true)}
              onRefreshQueue={fetchQueueStatus}
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
          />
        </div>
      </div>
    </div>
  );
}
