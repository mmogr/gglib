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
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  }) => void;
  selectedModelId: number | null;
  servers: ServerInfo[];
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
}

export function useMccMenuActions({
  onRegisterMenuActions,
  selectedModelId,
  servers,
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
      startServer: () => {
        if (selectedModelId) {
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
  ]);
}
