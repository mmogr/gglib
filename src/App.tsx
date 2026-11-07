import { useState, useEffect } from "react";
import { RouterProvider, useLocation, useNavigate } from "react-router";
import SettingsModal from "./components/SettingsModal";
import SetupWizard from "./components/SetupWizard";
import { ToastContainer } from "./components/Toast";
import { useDownloadToasts } from "./hooks/useDownloadToasts";
import { useFileDropGuard } from "./hooks/useFileDropGuard";
import { useOfferOutcomes } from "./app/useOfferOutcomes";
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
import { initDownloadEvents, cleanupDownloadEvents } from "./services/downloadsEvents";
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
  const { toasts, showToast, dismissToast } = useToastContext();
  // The settings dialog is open when the address says so, over the place;
  // opened and closed by replacing the entry, so Back never reopens it.
  const openSettings = settingsSection(location);

  // Initialize the registries: servers, proxy, remote, the model library
  // and the download queue
  useEffect(() => {
    initServerEvents();
    initProxyEvents();
    initRemoteEvents();
    initModelEvents();
    initDownloadEvents();
    return () => {
      cleanupServerEvents();
      cleanupProxyEvents();
      cleanupRemoteEvents();
      cleanupModelEvents();
      cleanupDownloadEvents();
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

  // How downloads ended, said on whatever page is open
  useDownloadToasts();

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
