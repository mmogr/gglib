import { useState, useEffect, useCallback, useRef } from "react";
import ModelControlCenterPage from "./pages/ModelControlCenterPage";
import Header from "./components/Header";
import SettingsModal from "./components/SettingsModal";
import LlamaInstallModal from "./components/LlamaInstallModal";
import { ToastContainer } from "./components/Toast";
import { useServers } from "./hooks/useServers";
import { useLlamaStatus } from "./hooks/useLlamaStatus";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ToastProvider, useToastContext } from "./contexts/ToastContext";
import { syncMenuStateSilent, listenToMenuEvents } from "./services/platform";
import { initServerEvents, cleanupServerEvents } from "./services/serverEvents";

/**
 * Inner app component that consumes ToastContext.
 * Separated so we can use useToastContext() after ToastProvider is mounted.
 */
function AppContent() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [showLlamaModal, setShowLlamaModal] = useState(false);
  // Sidebar visibility (for menu toggle, currently not visually implemented)
  const [, setIsSidebarVisible] = useState(true);
  const { servers, loadServers, stopServer } = useServers();
  const { toasts, showToast, dismissToast } = useToastContext();
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
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  } | null>(null);

  // Show llama install modal when needed (only for Tauri desktop app)
  useEffect(() => {
    if (!llamaLoading && llamaStatus && !llamaStatus.installed) {
      setShowLlamaModal(true);
    }
  }, [llamaLoading, llamaStatus]);

  // Initialize server lifecycle events (Tauri or SSE based on platform)
  useEffect(() => {
    initServerEvents();
    return () => cleanupServerEvents();
  }, []);

  // Close modal when installation completes
  useEffect(() => {
    if (installProgress?.status === 'completed') {
      setTimeout(() => {
        setShowLlamaModal(false);
        // Sync menu state after llama installation
        syncMenuStateSilent();
      }, 2000);
    }
  }, [installProgress?.status]);

  // Menu event listeners (desktop only - via platform helper)
  useEffect(() => {
    let cleanup: (() => void) | null = null;

    listenToMenuEvents({
      'menu:open-settings': () => setIsSettingsOpen(true),
      'menu:toggle-sidebar': () => setIsSidebarVisible(prev => !prev),
      'menu:add-model-file': () => menuActionsRef.current?.addModelFromFile?.(),
      'menu:refresh-models': () => menuActionsRef.current?.refreshModels?.(),
      'menu:start-server': () => menuActionsRef.current?.startServer?.(),
      'menu:stop-server': () => menuActionsRef.current?.stopServer?.(),
      'menu:remove-model': () => menuActionsRef.current?.removeModel?.(),
      'menu:install-llama': () => setShowLlamaModal(true),
      'menu:check-llama-status': () => checkLlamaStatus(),
      'menu:copy-to-clipboard': (payload) => {
        if (payload) {
          navigator.clipboard.writeText(payload).catch(console.error);
        }
      },
      'menu:proxy-stopped': () => syncMenuStateSilent(),
      'menu:start-proxy': () => setIsSettingsOpen(true),
    }).then(unsubscribe => {
      cleanup = unsubscribe;
    });

    return () => {
      cleanup?.();
    };
  }, [checkLlamaStatus]);

  // Handler for selecting a model from the header popover
  const handleSelectModelFromHeader = useCallback((modelId: number, view?: 'chat' | 'console') => {
    menuActionsRef.current?.selectModel?.(modelId, view);
  }, []);

  // Callback to register menu actions from ModelControlCenterPage
  const registerMenuActions = useCallback((actions: {
    refreshModels: () => void;
    addModelFromFile: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  }) => {
    menuActionsRef.current = actions;
  }, []);

  return (
    <SettingsProvider showToast={showToast}>
      <div className="app">
        <Header
          onOpenSettings={() => setIsSettingsOpen(true)}
          servers={servers}
          onStopServer={stopServer}
          onSelectModel={handleSelectModelFromHeader}
          onRefreshServers={loadServers}
        />
        <div className="app-body">
          <ModelControlCenterPage
            servers={servers}
            loadServers={loadServers}
            stopServer={stopServer}
            onRegisterMenuActions={registerMenuActions}
          />
        </div>
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
        <ToastContainer toasts={toasts} onDismiss={dismissToast} />
      </div>
    </SettingsProvider>
  );
}

/**
 * Root App component - wraps everything in providers.
 */
function App() {
  return (
    <ToastProvider>
      <AppContent />
    </ToastProvider>
  );
}

export default App;
