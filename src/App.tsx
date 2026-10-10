import { useState, useEffect, useCallback, useRef } from "react";
import ModelControlCenterPage from "./pages/ModelControlCenterPage";
import Header from "./components/Header";
import SettingsModal, { type SettingsTab } from "./components/SettingsModal";
import LlamaInstallModal from "./components/LlamaInstallModal";
import SetupWizard from "./components/SetupWizard";
import { ToastContainer } from "./components/Toast";
import { useServers } from "./hooks/useServers";
import { useLlamaStatus } from "./hooks/useLlamaStatus";
import { useFileDropGuard } from "./hooks/useFileDropGuard";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ToastProvider, useToastContext } from "./contexts/ToastContext";
import { ConfirmProvider } from "./contexts/ConfirmContext";
import { syncMenuStateSilent, listenToMenuEvents, MENU_EVENTS, appLogger } from "./services/platform";
import { initServerEvents, cleanupServerEvents } from "./services/serverEvents";
import { initProxyEvents, cleanupProxyEvents } from "./services/proxyEvents";
import { initRemoteEvents, cleanupRemoteEvents } from "./services/remoteEvents";
import { getTransport } from "./services/transport";
import { getSetupStatus } from "./services/transport/api/setup";
import { syncBuiltinTools } from "./services/tools";

/**
 * Inner app component that consumes ToastContext.
 * Separated so we can use useToastContext() after ToastProvider is mounted.
 */
function AppContent() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  // The tab Settings opens on: General from the header, System from a
  // prompt about the image runtime.
  const [settingsTab, setSettingsTab] = useState<SettingsTab | undefined>(undefined);
  const [showLlamaModal, setShowLlamaModal] = useState(false);
  const { servers, stopServer } = useServers();
  const { toasts, showToast, dismissToast } = useToastContext();
  const {
    status: llamaStatus,
    loading: llamaLoading,
    error: llamaError,
    checkStatus: checkLlamaStatus,
  } = useLlamaStatus();

  // Refs for menu event handlers (to access in ModelControlCenterPage)
  const menuActionsRef = useRef<{
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    showChat: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  } | null>(null);

  // Show the llama install modal when the daemon's machine has no llama.cpp
  useEffect(() => {
    if (!llamaLoading && llamaStatus && !llamaStatus.installed) {
      setShowLlamaModal(true);
    }
  }, [llamaLoading, llamaStatus]);

  // Initialize server and proxy lifecycle events
  useEffect(() => {
    initServerEvents();
    initProxyEvents();
    initRemoteEvents();
    return () => {
      cleanupServerEvents();
      cleanupProxyEvents();
      cleanupRemoteEvents();
    };
  }, []);

  // Sync built-in tool definitions from the backend into the tool registry
  useEffect(() => {
    syncBuiltinTools().catch(() => void 0);
  }, []);

  // A file dropped where nothing takes it must not open in place of the app
  useFileDropGuard();

  // An install completed: re-read the status, and close the modal once its
  // last line has had time to be read
  const handleLlamaInstalled = useCallback(() => {
    checkLlamaStatus();
    setTimeout(() => {
      setShowLlamaModal(false);
      // Sync menu state after llama installation
      syncMenuStateSilent();
    }, 2000);
  }, [checkLlamaStatus]);

  // Menu event listeners (desktop only - via platform helper)
  useEffect(() => {
    let cleanup: (() => void) | null = null;

    listenToMenuEvents({
      [MENU_EVENTS.OPEN_SETTINGS]: () => setIsSettingsOpen(true),
      [MENU_EVENTS.ADD_MODEL_FILE]: () => menuActionsRef.current?.addModelFromFile?.(),
      [MENU_EVENTS.SHOW_DOWNLOADS]: () => menuActionsRef.current?.showDownloads?.(),
      [MENU_EVENTS.SHOW_CHAT]: () => menuActionsRef.current?.showChat?.(),
      [MENU_EVENTS.REFRESH_MODELS]: () => menuActionsRef.current?.refreshModels?.(),
      [MENU_EVENTS.START_SERVER]: () => menuActionsRef.current?.startServer?.(),
      [MENU_EVENTS.STOP_SERVER]: () => menuActionsRef.current?.stopServer?.(),
      [MENU_EVENTS.REMOVE_MODEL]: () => menuActionsRef.current?.removeModel?.(),
      [MENU_EVENTS.INSTALL_LLAMA]: () => setShowLlamaModal(true),
      [MENU_EVENTS.CHECK_LLAMA_STATUS]: () => checkLlamaStatus(),
      [MENU_EVENTS.COPY_TO_CLIPBOARD]: (payload) => {
        if (payload) {
          navigator.clipboard.writeText(payload).catch((error) => 
            appLogger.error('component.app', 'Failed to copy to clipboard', { error })
          );
        }
      },
      [MENU_EVENTS.PROXY_STOPPED]: async () => {
        try {
          await getTransport().stopProxy();
          showToast('Proxy stopped', 'success');
        } catch (error) {
          showToast('Failed to stop proxy', 'error');
          appLogger.error('component.app', 'Failed to stop proxy', { error });
        }
      },
      [MENU_EVENTS.START_PROXY]: async () => {
        try {
          const status = await getTransport().startProxy();
          showToast(`Proxy started on port ${status.port}`, 'success');
        } catch (error) {
          showToast('Failed to start proxy', 'error');
          appLogger.error('component.app', 'Failed to start proxy', { error });
        }
      },
    }).then(unsubscribe => {
      cleanup = unsubscribe;
    });

    return () => {
      cleanup?.();
    };
  }, [checkLlamaStatus, showToast]);

  // Handler for selecting a model from the header popover
  const handleSelectModelFromHeader = useCallback((modelId: number, view?: 'chat' | 'console') => {
    menuActionsRef.current?.selectModel?.(modelId, view);
  }, []);

  // Callback to register menu actions from ModelControlCenterPage
  const registerMenuActions = useCallback((actions: {
    refreshModels: () => void;
    addModelFromFile: () => void;
    showDownloads: () => void;
    showChat: () => void;
    startServer: () => void;
    stopServer: () => void;
    removeModel: () => void;
    selectModel: (modelId: number, view?: 'chat' | 'console') => void;
  }) => {
    menuActionsRef.current = actions;
  }, []);

  return (
    <SettingsProvider showToast={showToast}>
      <div className="flex flex-col h-screen overflow-hidden">
        <Header
          onOpenSettings={() => {
            setSettingsTab(undefined);
            setIsSettingsOpen(true);
          }}
          servers={servers}
          onStopServer={stopServer}
          onSelectModel={handleSelectModelFromHeader}
        />
        <div className="flex-1 min-h-0 overflow-hidden flex">
          <ModelControlCenterPage
            servers={servers}
            stopServer={stopServer}
            onRegisterMenuActions={registerMenuActions}
            onOpenSystemSettings={() => {
              setSettingsTab('system');
              setIsSettingsOpen(true);
            }}
          />
        </div>
        {isSettingsOpen && (
          <SettingsModal
            isOpen={isSettingsOpen}
            initialTab={settingsTab}
            onClose={() => setIsSettingsOpen(false)}
          />
        )}
        {showLlamaModal && (
          <LlamaInstallModal
            canDownload={llamaStatus?.canDownload ?? false}
            error={llamaError}
            onSkip={() => setShowLlamaModal(false)}
            onInstalled={handleLlamaInstalled}
          />
        )}
        <ToastContainer toasts={toasts} onDismiss={dismissToast} />
      </div>
    </SettingsProvider>
  );
}

/**
 * Root App component - wraps everything in providers.
 * Shows setup wizard on first run before main app.
 */
function App() {
  const [setupDone, setSetupDone] = useState<boolean | null>(null);

  useEffect(() => {
    getSetupStatus()
      .then((status) => setSetupDone(status.setupCompleted))
      .catch(() => {
        // If we can't check, assume setup is done to avoid blocking
        setSetupDone(true);
      });
  }, []);

  // Still checking setup status
  if (setupDone === null) {
    return (
      <div className="fixed inset-0 bg-background flex items-center justify-center">
        <div className="text-text-secondary text-sm">Loading...</div>
      </div>
    );
  }

  // First run: show wizard
  if (!setupDone) {
    return (
      <ToastProvider>
        <ConfirmProvider>
          <SetupWizard onComplete={() => setSetupDone(true)} />
        </ConfirmProvider>
      </ToastProvider>
    );
  }

  return (
    <ToastProvider>
      <ConfirmProvider>
        <AppContent />
      </ConfirmProvider>
    </ToastProvider>
  );
}

export default App;
