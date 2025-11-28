import { Suspense, lazy, useState, useEffect, useCallback, useRef } from "react";
import ModelControlCenterPage from "./pages/ModelControlCenterPage";
import Header from "./components/Header";
import SettingsModal from "./components/SettingsModal";
import LlamaInstallModal from "./components/LlamaInstallModal";
import { useServers } from "./hooks/useServers";
import { useLlamaStatus } from "./hooks/useLlamaStatus";
import { isTauriApp, TauriService } from "./services/tauri";

// Tauri event listener (only imported in Tauri context)
let listen: ((event: string, handler: (event: any) => void) => Promise<() => void>) | null = null;
if (isTauriApp) {
  import("@tauri-apps/api/event").then((module) => {
    listen = module.listen;
  });
}

const ChatView = lazy(async () => {
  const module = await import("./components/ChatView");
  return { default: module.ChatView };
});

function App() {
  const [isChatOpen, setIsChatOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isWorkPanelVisible, setIsWorkPanelVisible] = useState(false);
  const [showLlamaModal, setShowLlamaModal] = useState(false);
  // Sidebar visibility (for menu toggle, currently not visually implemented)
  const [, setIsSidebarVisible] = useState(true);
  const { servers, loadServers, stopServer } = useServers();
  const { 
    status: llamaStatus, 
    loading: llamaLoading,
    error: llamaError,
    installing: llamaInstalling,
    installProgress,
    installLlama,
    checkStatus: checkLlamaStatus,
  } = useLlamaStatus();

  // Refs for menu event handlers (to access in ModelControlCenterPage)
  const menuActionsRef = useRef<{
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
  } | null>(null);

  // Show llama install modal when needed (only for Tauri desktop app)
  useEffect(() => {
    if (!llamaLoading && llamaStatus && !llamaStatus.installed) {
      setShowLlamaModal(true);
    }
  }, [llamaLoading, llamaStatus]);

  // Close modal when installation completes
  useEffect(() => {
    if (installProgress?.status === 'completed') {
      setTimeout(() => {
        setShowLlamaModal(false);
        // Sync menu state after llama installation
        TauriService.syncMenuState().catch(() => {});
      }, 2000);
    }
  }, [installProgress?.status]);

  // Menu event listeners (Tauri only)
  useEffect(() => {
    if (!isTauriApp || !listen) return;

    const unlisteners: (() => void)[] = [];

    const setupListeners = async () => {
      // Wait for listen to be loaded
      while (!listen) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }

      // Settings
      unlisteners.push(await listen("menu:open-settings", () => {
        setIsSettingsOpen(true);
      }));

      // Chat
      unlisteners.push(await listen("menu:show-chat", () => {
        setIsChatOpen(true);
      }));

      // Toggle sidebar
      unlisteners.push(await listen("menu:toggle-sidebar", () => {
        setIsSidebarVisible(prev => !prev);
      }));

      // Show downloads panel
      unlisteners.push(await listen("menu:show-downloads", () => {
        setIsWorkPanelVisible(true);
        // Trigger subtab change in ModelControlCenterPage
        menuActionsRef.current?.showDownloads?.();
      }));

      // Add model from file (triggers file dialog)
      unlisteners.push(await listen("menu:add-model-file", () => {
        menuActionsRef.current?.addModelFromFile?.();
      }));

      // Refresh models
      unlisteners.push(await listen("menu:refresh-models", () => {
        menuActionsRef.current?.refreshModels?.();
      }));

      // Start server for selected model
      unlisteners.push(await listen("menu:start-server", () => {
        menuActionsRef.current?.startServer?.();
      }));

      // Stop server for selected model
      unlisteners.push(await listen("menu:stop-server", () => {
        menuActionsRef.current?.stopServer?.();
      }));

      // Remove selected model
      unlisteners.push(await listen("menu:remove-model", () => {
        menuActionsRef.current?.removeModel?.();
      }));

      // Install llama.cpp
      unlisteners.push(await listen("menu:install-llama", () => {
        setShowLlamaModal(true);
      }));

      // Check llama.cpp status
      unlisteners.push(await listen("menu:check-llama-status", () => {
        checkLlamaStatus();
      }));

      // Copy to clipboard
      unlisteners.push(await listen("menu:copy-to-clipboard", (event: any) => {
        if (event.payload) {
          navigator.clipboard.writeText(event.payload).catch(console.error);
        }
      }));

      // Proxy events - just sync menu state
      unlisteners.push(await listen("menu:proxy-stopped", () => {
        TauriService.syncMenuState().catch(() => {});
      }));

      unlisteners.push(await listen("menu:start-proxy", () => {
        // Open settings to proxy section if user wants to configure
        setIsSettingsOpen(true);
      }));
    };

    setupListeners();

    return () => {
      unlisteners.forEach(unlisten => unlisten?.());
    };
  }, [checkLlamaStatus]);

  const toggleWorkPanel = () => setIsWorkPanelVisible((prev) => !prev);
  const showWorkPanel = () => setIsWorkPanelVisible(true);
  const hasRunningServers = servers.length > 0;

  // Callback to register menu actions from ModelControlCenterPage
  const registerMenuActions = useCallback((actions: {
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
  }) => {
    menuActionsRef.current = actions;
  }, []);

  return (
    <div className="app">
      <Header
        onOpenChat={() => setIsChatOpen(true)}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onToggleWorkPanel={toggleWorkPanel}
        isWorkPanelVisible={isWorkPanelVisible}
        isModelRunning={hasRunningServers}
      />
      <div className="app-body">
        <ModelControlCenterPage
          servers={servers}
          loadServers={loadServers}
          stopServer={stopServer}
          isWorkPanelVisible={isWorkPanelVisible}
          onShowWorkPanel={showWorkPanel}
          onRegisterMenuActions={registerMenuActions}
        />
      </div>
      {isChatOpen && (
        <Suspense fallback={<div className="chat-loading">Preparing chat experience…</div>}>
          <ChatView onClose={() => setIsChatOpen(false)} />
        </Suspense>
      )}
      {isSettingsOpen && (
        <SettingsModal isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
      )}
      <LlamaInstallModal
        isOpen={showLlamaModal}
        canDownload={llamaStatus?.canDownload ?? false}
        installing={llamaInstalling}
        progress={installProgress}
        error={llamaError}
        onInstall={installLlama}
        onSkip={() => setShowLlamaModal(false)}
      />
    </div>
  );
}

export default App;
