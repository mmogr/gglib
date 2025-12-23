import { FC, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { useModelsDirectory } from "../hooks/useModelsDirectory";
import { useSettings } from "../hooks/useSettings";
import { useMcpServers } from "../hooks/useMcpServers";
import { UpdateSettingsRequest } from "../types";
import type { McpServerInfo } from "../services/clients/mcp";
import { McpServersPanel } from "./McpServersPanel";
import { AddMcpServerModal } from "./AddMcpServerModal";
import { GeneralSettings } from "./SettingsModal/GeneralSettings";
import { Modal } from "./ui/Modal";
import styles from "./SettingsModal.module.css";

type SettingsTab = "general" | "mcp";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const sourceLabels: Record<string, string> = {
  explicit: "Custom path (CLI/UI override)",
  env: "Configured via .env",
  default: "Default (~/.local/share/llama_models)",
};

export const SettingsModal: FC<SettingsModalProps> = ({ isOpen, onClose }) => {
  const { info, loading: loadingDir, saving: savingDir, error: dirError, refresh: refreshDir, save: saveDir } = useModelsDirectory();
  const { settings, loading: loadingSettings, saving: savingSettings, error: settingsError, refresh: refreshSettings, save: saveSettings } = useSettings();
  
  const [pathInput, setPathInput] = useState("");
  const [contextSizeInput, setContextSizeInput] = useState("");
  const [proxyPortInput, setProxyPortInput] = useState("");
  const [serverPortInput, setServerPortInput] = useState("");
  const [maxQueueSizeInput, setMaxQueueSizeInput] = useState("");
  const [titlePromptInput, setTitlePromptInput] = useState("");
  const [maxToolIterationsInput, setMaxToolIterationsInput] = useState("");
  const [maxStagnationStepsInput, setMaxStagnationStepsInput] = useState("");
  const [showFitIndicators, setShowFitIndicators] = useState(true);
  const [isAdvancedOpen, setIsAdvancedOpen] = useState(false);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  
  // MCP Server modal state
  const [showAddMcpModal, setShowAddMcpModal] = useState(false);
  const [editingMcpServer, setEditingMcpServer] = useState<McpServerInfo | null>(null);
  const { addServer: addMcpServer, updateServer: updateMcpServer } = useMcpServers();

  const loading = loadingDir || loadingSettings;
  const saving = savingDir || savingSettings;
  const error = dirError || settingsError;

  useEffect(() => {
    if (info?.path) {
      setPathInput(info.path);
    }
  }, [info]);

  useEffect(() => {
    if (settings) {
      setContextSizeInput(settings.default_context_size?.toString() || "");
      setProxyPortInput(settings.proxy_port?.toString() || "");
      setServerPortInput(settings.llama_base_port?.toString() || "");
      setMaxQueueSizeInput(settings.max_download_queue_size?.toString() || "");
      setTitlePromptInput(settings.title_generation_prompt || "");
      setMaxToolIterationsInput(settings.max_tool_iterations?.toString() || "");
      setMaxStagnationStepsInput(settings.max_stagnation_steps?.toString() || "");
      setShowFitIndicators(settings.show_memory_fit_indicators !== false);
    }
  }, [settings]);

  const handleSubmit = useCallback(
    async (event: FormEvent) => {
      event.preventDefault();
      setSuccessMessage(null);
      
      try {
        // Update models directory if changed
        if (pathInput.trim() && pathInput !== info?.path) {
          await saveDir(pathInput.trim());
        }

        // Helper function to parse numeric input
        const parseNumericInput = (input: string): number | null => {
          if (!input.trim()) return null;
          const parsed = parseInt(input.trim(), 10);
          return isNaN(parsed) ? null : parsed;
        };

        // Update other settings
        const updates: UpdateSettingsRequest = {
          default_context_size: parseNumericInput(contextSizeInput),
          proxy_port: parseNumericInput(proxyPortInput),
          llama_base_port: parseNumericInput(serverPortInput),
          max_download_queue_size: parseNumericInput(maxQueueSizeInput),
          title_generation_prompt: titlePromptInput.trim() || null,
          max_tool_iterations: parseNumericInput(maxToolIterationsInput),
          max_stagnation_steps: parseNumericInput(maxStagnationStepsInput),
          show_memory_fit_indicators: showFitIndicators,
        };

        // Check if any updates were made
        const hasUpdates = 
          updates.default_context_size !== undefined ||
          updates.proxy_port !== undefined ||
          updates.llama_base_port !== undefined ||
          updates.max_download_queue_size !== undefined ||
          updates.title_generation_prompt !== undefined ||
          updates.max_tool_iterations !== undefined ||
          updates.max_stagnation_steps !== undefined ||
          updates.show_memory_fit_indicators !== undefined;

        if (hasUpdates) {
          await saveSettings(updates);
        }

        setSuccessMessage("Settings updated successfully");
      } catch (err) {
        console.error("Failed to update settings", err);
      }
    },
    [
      pathInput,
      contextSizeInput,
      proxyPortInput,
      serverPortInput,
      maxQueueSizeInput,
      titlePromptInput,
      maxToolIterationsInput,
      maxStagnationStepsInput,
      showFitIndicators,
      info,
      saveDir,
      saveSettings,
    ]
  );

  const handleReset = useCallback(() => {
    if (info?.default_path) {
      setPathInput(info.default_path);
    }
    if (settings) {
      setContextSizeInput(settings.default_context_size?.toString() || "4096");
      setProxyPortInput(settings.proxy_port?.toString() || "8080");
      setServerPortInput(settings.llama_base_port?.toString() || "9000");
      setMaxQueueSizeInput(settings.max_download_queue_size?.toString() || "10");
      setTitlePromptInput(""); // Reset to default (empty uses DEFAULT_TITLE_GENERATION_PROMPT)
      setShowFitIndicators(true); // Default is enabled
    }
  }, [info, settings]);

  const handleRefresh = useCallback(() => {
    refreshDir();
    refreshSettings();
  }, [refreshDir, refreshSettings]);

  const sourceDescription = useMemo(() => {
    if (!info) {
      return null;
    }
    return sourceLabels[info.source] || info.source;
  }, [info]);

  return (
    <>
      <Modal
        open={isOpen}
        onClose={onClose}
        title="Settings"
        description="Configure download paths, ports, and MCP servers."
        size="lg"
        preventClose={saving}
      >
        {/* Tab Navigation */}
        <div className={styles.tabs}>
          <button
            type="button"
            className={`${styles.tab} ${activeTab === "general" ? styles.tabActive : ""}`}
            onClick={() => setActiveTab("general")}
          >
            General
          </button>
          <button
            type="button"
            className={`${styles.tab} ${activeTab === "mcp" ? styles.tabActive : ""}`}
            onClick={() => setActiveTab("mcp")}
          >
            MCP Servers
          </button>
        </div>

        {/* General Settings Tab */}
        {activeTab === "general" && (
          <GeneralSettings
            pathInput={pathInput}
            setPathInput={setPathInput}
            info={info}
            sourceDescription={sourceDescription}
            contextSizeInput={contextSizeInput}
            setContextSizeInput={setContextSizeInput}
            proxyPortInput={proxyPortInput}
            setProxyPortInput={setProxyPortInput}
            serverPortInput={serverPortInput}
            setServerPortInput={setServerPortInput}
            maxQueueSizeInput={maxQueueSizeInput}
            setMaxQueueSizeInput={setMaxQueueSizeInput}
            showFitIndicators={showFitIndicators}
            setShowFitIndicators={setShowFitIndicators}
            isAdvancedOpen={isAdvancedOpen}
            setIsAdvancedOpen={setIsAdvancedOpen}
            maxToolIterationsInput={maxToolIterationsInput}
            setMaxToolIterationsInput={setMaxToolIterationsInput}
            maxStagnationStepsInput={maxStagnationStepsInput}
            setMaxStagnationStepsInput={setMaxStagnationStepsInput}
            titlePromptInput={titlePromptInput}
            setTitlePromptInput={setTitlePromptInput}
            onSubmit={handleSubmit}
            onReset={handleReset}
            onRefresh={handleRefresh}
            onClose={onClose}
            loading={loading}
            saving={saving}
            error={error}
            successMessage={successMessage}
          />
        )}

        {/* MCP Servers Tab */}
        {activeTab === "mcp" && (
          <>
            <McpServersPanel
              onAddServer={() => {
                setEditingMcpServer(null);
                setShowAddMcpModal(true);
              }}
              onEditServer={(server) => {
                setEditingMcpServer(server);
                setShowAddMcpModal(true);
              }}
            />
            {showAddMcpModal && (
              <AddMcpServerModal
                isOpen={showAddMcpModal}
                editingServer={editingMcpServer ?? undefined}
                onClose={() => {
                  setShowAddMcpModal(false);
                  setEditingMcpServer(null);
                }}
                onSave={async (serverData) => {
                  if (editingMcpServer) {
                    // Update existing server with new data
                    await updateMcpServer(editingMcpServer.server.id, serverData);
                  } else {
                    await addMcpServer(serverData);
                  }
                  setShowAddMcpModal(false);
                  setEditingMcpServer(null);
                }}
              />
            )}
          </>
        )}
      </Modal>
    </>
  );
};

export default SettingsModal;
