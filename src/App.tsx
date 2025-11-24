import { Suspense, lazy, useState } from "react";
import ModelControlCenterPage from "./pages/ModelControlCenterPage";
import Header from "./components/Header";
import SettingsModal from "./components/SettingsModal";
import { useServers } from "./hooks/useServers";

const ChatView = lazy(async () => {
  const module = await import("./components/ChatView");
  return { default: module.ChatView };
});

function App() {
  const [isChatOpen, setIsChatOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isWorkPanelVisible, setIsWorkPanelVisible] = useState(false);
  const { servers, loadServers } = useServers();

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
    </div>
  );
}

export default App;
