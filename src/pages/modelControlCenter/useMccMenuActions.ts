import { useEffect } from 'react';
import { syncMenuStateSilent } from '../../services/platform';
import type { ServerInfo } from '../../types';
import type { SidebarTabId } from '../../components/ModelLibraryPanel/SidebarTabs';
import type { AddDownloadSubTab } from '../../components/ModelLibraryPanel/AddDownloadContent';

interface UseMccMenuActionsArgs {
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
  selectedModelId: number | null;
  servers: ServerInfo[];
  models: Array<{ id?: number; name?: string }>;
  loadServers: () => Promise<void>;
  stopServer: (modelId: number) => Promise<void>;
  removeModel: (id: number, force?: boolean) => Promise<void>;
  selectModel: (id: number | null) => void;
  setSidebarTab: (tab: SidebarTabId) => void;
  setActiveSubTab: (tab: AddDownloadSubTab) => void;
  triggerFilePicker: () => void;
  refreshAll: () => Promise<void>;
  chatSessionModelId: number | null;
  closeChatSession: () => void;
  openChatSession: (modelId: number, view: 'chat' | 'console') => void;
  onOpenServeModal?: () => void;
  showToast: (message: string, type?: 'info' | 'success' | 'warning' | 'error', duration?: number) => void;
}

export function useMccMenuActions({
  onRegisterMenuActions,
  selectedModelId,
  servers,
  models,
  loadServers,
  stopServer,
  removeModel,
  selectModel,
  setSidebarTab,
  setActiveSubTab,
  triggerFilePicker,
  refreshAll,
  chatSessionModelId,
  closeChatSession,
  openChatSession,
  onOpenServeModal,
  showToast,
}: UseMccMenuActionsArgs) {
  useEffect(() => {
    if (!onRegisterMenuActions) return;

    onRegisterMenuActions({
      refreshModels: () => {
        refreshAll();
      },
      addModelFromFile: () => {
        setSidebarTab('add');
        setActiveSubTab('add');
        triggerFilePicker();
      },
      showDownloads: () => {
        setSidebarTab('add');
        setActiveSubTab('browse');
      },
      showChat: () => {
        // Open chat for any running server, preferring the selected model's server
        let serverToOpen = null;
        
        if (selectedModelId) {
          // Try to find server for selected model
          serverToOpen = servers.find((s) => s.model_id === selectedModelId);
        }
        
        if (!serverToOpen && servers.length > 0) {
          // Fall back to first running server
          serverToOpen = servers[0];
        }
        
        if (serverToOpen) {
          openChatSession(serverToOpen.model_id, 'chat');
        } else {
          // No servers running - show helpful message
          showToast('No servers are currently running. Start a server first to use chat.', 'warning');
        }
      },
      startServer: () => {
        if (selectedModelId && onOpenServeModal) {
          // Open the serve modal for the selected model
          onOpenServeModal();
        } else if (selectedModelId) {
          // Fallback: if no modal callback, just load servers (old behavior)
          loadServers();
        }
      },
      stopServer: async () => {
        if (!selectedModelId) return;
        const runningServer = servers.find((s) => s.model_id === selectedModelId);
        if (runningServer) {
          await stopServer(selectedModelId);
          if (chatSessionModelId === selectedModelId) {
            closeChatSession();
          }
          syncMenuStateSilent();
        }
      },
      removeModel: async () => {
        if (!selectedModelId) return;
        
        // Find model name for confirmation
        const selectedModel = models.find(m => m.id === selectedModelId);
        const modelName = selectedModel?.name || 'this model';
        
        // Show confirmation dialog
        const confirmed = window.confirm(
          `Are you sure you want to remove "${modelName}" from the library?\n\nThis will not delete the model file from disk.`
        );
        
        if (!confirmed) return;
        
        await removeModel(selectedModelId, false);
        syncMenuStateSilent();
      },
      selectModel: (modelId: number, view?: 'chat' | 'console') => {
        if (view) {
          const server = servers.find((s) => s.model_id === modelId);
          if (server) {
            openChatSession(modelId, view);
          }
        } else {
          selectModel(modelId);
        }
      },
    });
  }, [
    onRegisterMenuActions,
    selectedModelId,
    servers,
    models,
    loadServers,
    stopServer,
    removeModel,
    selectModel,
    setSidebarTab,
    setActiveSubTab,
    triggerFilePicker,
    refreshAll,
    chatSessionModelId,
    closeChatSession,
    openChatSession,
    onOpenServeModal,
    showToast,
  ]);
}
