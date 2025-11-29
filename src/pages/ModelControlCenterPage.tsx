import { useState, useRef, useCallback, useEffect } from 'react';
import { useModels } from '../hooks/useModels';
import { useTags } from '../hooks/useTags';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import ModelInspectorPanel from '../components/ModelInspectorPanel/ModelInspectorPanel';
import WorkPanel from '../components/WorkPanel/WorkPanel';
import { TauriService } from '../services/tauri';
import { ServerInfo } from '../types';
import { SidebarTabId } from '../components/ModelLibraryPanel/SidebarTabs';
import { AddDownloadSubTab } from '../components/ModelLibraryPanel/AddDownloadContent';
import './ModelControlCenterPage.css';

export type WorkPanelTab = 'add-download' | 'runs';

interface ModelControlCenterPageProps {
  servers: ServerInfo[];
  loadServers: () => Promise<void>;
  stopServer: (modelId: number) => Promise<void>;
  isWorkPanelVisible: boolean;
  onShowWorkPanel: () => void;
  onRegisterMenuActions?: (actions: {
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
  }) => void;
}

export default function ModelControlCenterPage({
  servers,
  loadServers,
  stopServer,
  isWorkPanelVisible,
  onShowWorkPanel,
  onRegisterMenuActions,
}: ModelControlCenterPageProps) {
  const { models, selectedModel, selectedModelId, loading, error, loadModels, selectModel, addModel, removeModel, updateModel } = useModels();
  const { tags, addTagToModel, removeTagFromModel, getModelTags } = useTags();

  const [activeTab, setActiveTab] = useState<WorkPanelTab>('add-download');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTags, setSelectedTags] = useState<string[]>([]);
  const [activeSubTab, setActiveSubTab] = useState<AddDownloadSubTab>('browse');
  
  // Sidebar tab state (for the new tabbed sidebar)
  const [sidebarTab, setSidebarTab] = useState<SidebarTabId>('models');
  
  // Ref for file input (for menu-triggered file add)
  const fileInputRef = useRef<HTMLInputElement>(null);
  
  // Panel width state (percentages)
  const [leftPanelWidth, setLeftPanelWidth] = useState(45);
  const [centerPanelWidth, setCenterPanelWidth] = useState(30);
  
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef<number | null>(null); // 0 for left, 1 for center

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
      });
    }
  }, [onRegisterMenuActions, loadModels, selectedModelId, servers, stopServer, removeModel, isWorkPanelVisible, onShowWorkPanel, loadServers]);

  // Handle resize
  const handleMouseDown = useCallback((panelIndex: number) => (e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingRef.current = panelIndex;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  useEffect(() => {
    let rafId: number | null = null;
    
    const handleMouseMove = (e: MouseEvent) => {
      if (isDraggingRef.current === null || !layoutRef.current) return;
      
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      
      rafId = requestAnimationFrame(() => {
        if (!layoutRef.current) return;
        
        const rect = layoutRef.current.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = (x / rect.width) * 100;
        
        if (isDraggingRef.current === 0) {
          // Resizing left panel
          const newLeftWidth = Math.max(20, Math.min(70, percentage));
          setLeftPanelWidth(newLeftWidth);
        } else if (isDraggingRef.current === 1 && isWorkPanelVisible) {
          // Resizing center panel (only when work panel is visible)
          const remainingWidth = 100 - leftPanelWidth;
          const newCenterWidth = Math.max(15, Math.min(remainingWidth - 15, percentage - leftPanelWidth));
          setCenterPanelWidth(newCenterWidth);
        }
      });
    };
    
    const handleMouseUp = () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      isDraggingRef.current = null;
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
  }, [leftPanelWidth, isWorkPanelVisible]);

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

  const handleShowWorkPanel = (tab: WorkPanelTab, subtab?: 'add' | 'download') => {
    setActiveTab(tab);
    if (subtab) {
      setActiveSubTab(subtab);
    }
    if (!isWorkPanelVisible) {
      onShowWorkPanel();
    }
  };

  const handleSelectModel = (modelId: number) => {
    selectModel(modelId);
  };

  return (
    <div className="model-control-center">
      <div 
        ref={layoutRef}
        className={`mcc-layout ${isWorkPanelVisible ? 'work-panel-visible' : 'work-panel-hidden'}`}
        style={{
          gridTemplateColumns: isWorkPanelVisible 
            ? `${leftPanelWidth}% ${centerPanelWidth}% ${100 - leftPanelWidth - centerPanelWidth}%`
            : `${leftPanelWidth}% ${100 - leftPanelWidth}%`
        }}
      >
        {/* Left Panel: Model Library */}
        <div className="grid-panel-container">
          <ModelLibraryPanel
            models={filteredModels}
            selectedModelId={selectedModelId}
            onSelectModel={selectModel}
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
            onSubTabChange={setActiveSubTab}
            activeTab={sidebarTab}
            onTabChange={setSidebarTab}
          />
          <div 
            className="resize-handle" 
            onMouseDown={handleMouseDown(0)}
          />
        </div>

        {/* Center Panel: Model Inspector */}
        <div className="grid-panel-container">
          <ModelInspectorPanel
            model={selectedModel}
            onStartServer={loadServers}
            onStopServer={stopServer}
            servers={servers}
            onRemoveModel={removeModel}
            onUpdateModel={updateModel}
            onAddTag={addTagToModel}
            onRemoveTag={removeTagFromModel}
            getModelTags={getModelTags}
          />
          {isWorkPanelVisible && (
            <div 
              className="resize-handle" 
              onMouseDown={handleMouseDown(1)}
            />
          )}
        </div>

        {/* Right Panel: Work Panel */}
        {isWorkPanelVisible && (
          <WorkPanel
            activeTab={activeTab}
            onTabChange={setActiveTab}
            onModelAdded={handleModelAdded}
            onModelDownloaded={handleModelDownloaded}
            servers={servers}
            onStopServer={stopServer}
            onRefreshServers={loadServers}
            onSelectModel={handleSelectModel}
            activeSubTab={activeSubTab}
            onSubTabChange={setActiveSubTab}
          />
        )}
      </div>
    </div>
  );
}
