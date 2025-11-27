import { Suspense, lazy, useState, useEffect } from "react";
import ModelControlCenterPage from "./pages/ModelControlCenterPage";
import Header from "./components/Header";
import SettingsModal from "./components/SettingsModal";
import LlamaInstallModal from "./components/LlamaInstallModal";
import { useServers } from "./hooks/useServers";
import { useLlamaStatus } from "./hooks/useLlamaStatus";

const ChatView = lazy(async () => {
  const module = await import("./components/ChatView");
  return { default: module.ChatView };
});

function App() {
  const [isChatOpen, setIsChatOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isWorkPanelVisible, setIsWorkPanelVisible] = useState(false);
  const [showLlamaModal, setShowLlamaModal] = useState(false);
  const { servers, loadServers, stopServer } = useServers();
  const { 
    status: llamaStatus, 
    loading: llamaLoading,
    error: llamaError,
    installing: llamaInstalling,
    installProgress,
    installLlama,
  } = useLlamaStatus();

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
      }, 2000);
    }
  }, [installProgress?.status]);

  const toggleWorkPanel = () => setIsWorkPanelVisible((prev) => !prev);
  const showWorkPanel = () => setIsWorkPanelVisible(true);
  const hasRunningServers = servers.length > 0;

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
