import { useState, useEffect, useCallback } from "react";
import { RouterProvider, useLocation, useNavigate } from "react-router";
import SettingsModal from "./components/SettingsModal";
import LlamaInstallModal from "./components/LlamaInstallModal";
import SetupWizard from "./components/SetupWizard";
import { ToastContainer } from "./components/Toast";
import { useLlamaStatus } from "./hooks/useLlamaStatus";
import { useFileDropGuard } from "./hooks/useFileDropGuard";
import { useOfferOutcomes } from "./components/remote/useOfferOutcomes";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ToastProvider, useToastContext } from "./contexts/ToastContext";
import { ConfirmProvider } from "./contexts/ConfirmContext";
import { startMenuBridge } from "./app/menuBridge";
import { settingsSection, withoutSettings } from "./app/paths";
import { createAppRouter } from "./app/router";
import { Shell } from "./app/Shell";
import { initServerEvents, cleanupServerEvents } from "./services/serverEvents";
import { initProxyEvents, cleanupProxyEvents } from "./services/proxyEvents";
import { initRemoteEvents, cleanupRemoteEvents } from "./services/remoteEvents";
import { initModelEvents, cleanupModelEvents } from "./services/modelEvents";
import { getSetupStatus } from "./services/transport/api/setup";
import { syncBuiltinTools } from "./services/tools";

/**
 * The layout every place sits in: the shell (the sidebar and the place the
 * address names) and the dialogs drawn over whichever place is open.
 * Separated from `App` so it can use the toast context and the router.
 */
export function AppContent() {
  const location = useLocation();
  const navigate = useNavigate();
  const [showLlamaModal, setShowLlamaModal] = useState(false);
  const { toasts, showToast, dismissToast } = useToastContext();
  const {
    status: llamaStatus,
    loading: llamaLoading,
    error: llamaError,
    checkStatus: checkLlamaStatus,
  } = useLlamaStatus();
  // The settings dialog is open when the address says so, over the place;
  // opened and closed by replacing the entry, so Back never reopens it.
  const openSettings = settingsSection(location);

  // Show the llama install modal when the daemon's machine has no llama.cpp,
  // or one older than the pinned release, offered as an update
  useEffect(() => {
    if (!llamaLoading && llamaStatus && (!llamaStatus.installed || llamaStatus.update)) {
      setShowLlamaModal(true);
    }
  }, [llamaLoading, llamaStatus]);

  // Initialize the registries: servers, proxy, remote and the model library
  useEffect(() => {
    initServerEvents();
    initProxyEvents();
    initRemoteEvents();
    initModelEvents();
    return () => {
      cleanupServerEvents();
      cleanupProxyEvents();
      cleanupRemoteEvents();
      cleanupModelEvents();
    };
  }, []);

  // Sync built-in tool definitions from the backend into the tool registry
  useEffect(() => {
    syncBuiltinTools().catch(() => void 0);
  }, []);

  // A file dropped where nothing takes it must not open in place of the app
  useFileDropGuard();

  // How a pairing code ended, said on whatever page is open
  useOfferOutcomes();

  // An install completed: re-read the status, and close the modal once its
  // last line has had time to be read
  const handleLlamaInstalled = useCallback(() => {
    checkLlamaStatus();
    setTimeout(() => setShowLlamaModal(false), 2000);
  }, [checkLlamaStatus]);

  // The native menu (desktop only), navigating and acting through the router
  // and the registries; its proxy messages are toasted here.
  useEffect(() => startMenuBridge({ notify: showToast }), [showToast]);

  return (
    <SettingsProvider showToast={showToast}>
      <>
        <Shell />
        {openSettings && (
          <SettingsModal
            isOpen
            initialTab={openSettings}
            onClose={() => navigate(withoutSettings(location), { replace: true })}
          />
        )}
        {showLlamaModal && (
          <LlamaInstallModal
            canDownload={llamaStatus?.canDownload ?? false}
            update={llamaStatus?.installed ? llamaStatus.update : null}
            error={llamaError}
            onSkip={() => setShowLlamaModal(false)}
            onInstalled={handleLlamaInstalled}
          />
        )}
        <ToastContainer toasts={toasts} onDismiss={dismissToast} />
      </>
    </SettingsProvider>
  );
}

/**
 * Root App component - wraps everything in providers.
 * Shows setup wizard on first run before main app; past it, the router.
 */
function App() {
  const [setupDone, setSetupDone] = useState<boolean | null>(null);
  // Created once, after main.tsx has taken the daemon's token out of the address.
  const [router] = useState(() => createAppRouter(<AppContent />));

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
        <RouterProvider router={router} />
      </ConfirmProvider>
    </ToastProvider>
  );
}

export default App;
