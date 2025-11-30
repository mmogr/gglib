import { useState, useRef, useCallback, useEffect } from 'react';
import { useModels } from '../hooks/useModels';
import { useTags } from '../hooks/useTags';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import ModelInspectorPanel from '../components/ModelInspectorPanel/ModelInspectorPanel';
import ChatPage from './ChatPage';
import { TauriService } from '../services/tauri';
import { ServerInfo, HfModelSummary } from '../types';
import { SidebarTabId } from '../components/ModelLibraryPanel/SidebarTabs';
import { AddDownloadSubTab } from '../components/ModelLibraryPanel/AddDownloadContent';
import './ModelControlCenterPage.css';

interface ChatSession {
  serverPort: number;
  modelId: number;
  modelName: string;
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
    selectModel: (modelId: number) => void;
  }) => void;
}

export default function ModelControlCenterPage({
  servers,
  loadServers,
  stopServer,
  onRegisterMenuActions,
}: ModelControlCenterPageProps) {
  const { models, selectedModel, selectedModelId, loading, error, loadModels, selectModel, addModel, removeModel, updateModel } = useModels();
  const { tags, addTagToModel, removeTagFromModel, getModelTags } = useTags();

  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [activeSubTab, setActiveSubTab] = useState<AddDownloadSubTab>('download');
  
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
          loadModels();
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
        selectModel: (modelId: number) => {
          selectModel(modelId);
        },
      });
    }
  }, [onRegisterMenuActions, loadModels, selectedModelId, servers, stopServer, removeModel, loadServers, selectModel, chatSession]);

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

  // Filter models based on search and tags
  const filteredModels = models.filter(model => {
    const matchesSearch = !searchQuery || 
      model.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      model.architecture?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      model.hf_repo_id?.toLowerCase().includes(searchQuery.toLowerCase());
    
    const matchesTags = selectedTags.length === 0 || 
      (model.tags && selectedTags.some(tag => model.tags!.includes(tag)));
    
    return matchesSearch && matchesTags;
  });

  const handleModelAdded = async (filePath: string) => {
    if (filePath) {
      await addModel(filePath);
    } else {
      await loadModels();
    }
  };

  const handleModelDownloaded = async () => {
    await loadModels();
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

  // Handler for sidebar tab changes - clears HF selection when leaving HF browser context
  const handleSidebarTabChange = (tab: SidebarTabId) => {
    setSidebarTab(tab);
    // Clear HF model selection when switching away from the Add Models tab
    if (tab !== 'add') {
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
      <ChatPage
        serverPort={chatSession.serverPort}
        modelName={chatSession.modelName}
        onClose={handleCloseChat}
      />
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
            selectedTags={selectedTags}
            onTagFilterChange={setSelectedTags}
            servers={servers}
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
        <div className="grid-panel-container">
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
            onDownloadCompleted={loadModels}
          />
        </div>
      </div>
    </div>
  );
}
