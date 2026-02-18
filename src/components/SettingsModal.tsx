import { FC, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { appLogger } from '../services/platform';
import { useModelsDirectory } from "../hooks/useModelsDirectory";
import { useSettings } from "../hooks/useSettings";
import { useMcpServers } from "../hooks/useMcpServers";
import { useModels } from "../hooks/useModels";
import { UpdateSettingsRequest, InferenceConfig } from "../types";
import type { McpServerInfo } from "../services/clients/mcp";
import { McpServersPanel } from "./McpServersPanel";
import { AddMcpServerModal } from "./AddMcpServerModal";
import { GeneralSettings } from "./SettingsModal/GeneralSettings";
import { VoiceSettings } from "./SettingsModal/VoiceSettings";
import { Modal } from "./ui/Modal";
import { cn } from '../utils/cn';

type SettingsTab = "general" | "mcp" | "voice";

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
  const { models, loading: loadingModels } = useModels();
  
  const [pathInput, setPathInput] = useState("");
  const [contextSizeInput, setContextSizeInput] = useState("");
  const [proxyPortInput, setProxyPortInput] = useState("");
  const [serverPortInput, setServerPortInput] = useState("");
  const [maxQueueSizeInput, setMaxQueueSizeInput] = useState("");
  const [titlePromptInput, setTitlePromptInput] = useState("");
  const [maxToolIterationsInput, setMaxToolIterationsInput] = useState("");
  const [maxStagnationStepsInput, setMaxStagnationStepsInput] = useState("");
  const [showFitIndicators, setShowFitIndicators] = useState(true);
  const [defaultModelInput, setDefaultModelInput] = useState("");
  const [inferenceDefaultsInput, setInferenceDefaultsInput] = useState<InferenceConfig | undefined>(undefined);
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
      setContextSizeInput(settings.defaultContextSize?.toString() || "");
      setProxyPortInput(settings.proxyPort?.toString() || "");
      setServerPortInput(settings.llamaBasePort?.toString() || "");
      setMaxQueueSizeInput(settings.maxDownloadQueueSize?.toString() || "");
      setTitlePromptInput(settings.titleGenerationPrompt || "");
      setMaxToolIterationsInput(settings.maxToolIterations?.toString() || "");
      setMaxStagnationStepsInput(settings.maxStagnationSteps?.toString() || "");
      setShowFitIndicators(settings.showMemoryFitIndicators !== false);
      setDefaultModelInput(settings.defaultModelId?.toString() || "");
      setInferenceDefaultsInput(settings.inferenceDefaults || undefined);
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
          defaultContextSize: parseNumericInput(contextSizeInput),
          proxyPort: parseNumericInput(proxyPortInput),
          llamaBasePort: parseNumericInput(serverPortInput),
          maxDownloadQueueSize: parseNumericInput(maxQueueSizeInput),
          titleGenerationPrompt: titlePromptInput.trim() || null,
          maxToolIterations: parseNumericInput(maxToolIterationsInput),
          maxStagnationSteps: parseNumericInput(maxStagnationStepsInput),
          showMemoryFitIndicators: showFitIndicators,
          defaultModelId: parseNumericInput(defaultModelInput),
          inferenceDefaults: inferenceDefaultsInput,
        };

        // Check if any updates were made
        const hasUpdates = 
          updates.defaultContextSize !== undefined ||
          updates.proxyPort !== undefined ||
          updates.llamaBasePort !== undefined ||
          updates.maxDownloadQueueSize !== undefined ||
          updates.titleGenerationPrompt !== undefined ||
          updates.maxToolIterations !== undefined ||
          updates.maxStagnationSteps !== undefined ||
          updates.showMemoryFitIndicators !== undefined ||
          updates.defaultModelId !== undefined ||
          updates.inferenceDefaults !== undefined;

        if (hasUpdates) {
          await saveSettings(updates);
        }

        setSuccessMessage("Settings updated successfully");
      } catch (err) {
        appLogger.error('component.settings', 'Failed to update settings', { error: err });
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
      defaultModelInput,
      inferenceDefaultsInput,
      info,
      saveDir,
      saveSettings,
    ]
  );

  const handleReset = useCallback(() => {
    if (info?.defaultPath) {
      setPathInput(info.defaultPath);
    }
    if (settings) {
      setContextSizeInput(settings.defaultContextSize?.toString() || "4096");
      setProxyPortInput(settings.proxyPort?.toString() || "8080");
      setServerPortInput(settings.llamaBasePort?.toString() || "9000");
      setMaxQueueSizeInput(settings.maxDownloadQueueSize?.toString() || "10");
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
        <div className="flex gap-xs border-b border-border mb-md">
          <button
            type="button"
            className={cn(
              'px-md py-sm bg-none border-none border-b-2 border-b-transparent text-text-secondary text-sm font-semibold cursor-pointer transition-all duration-200 hover:text-text',
              activeTab === "general" && 'text-primary border-b-primary',
            )}
            onClick={() => setActiveTab("general")}
          >
            General
          </button>
          <button
            type="button"
            className={cn(
              'px-md py-sm bg-none border-none border-b-2 border-b-transparent text-text-secondary text-sm font-semibold cursor-pointer transition-all duration-200 hover:text-text',
              activeTab === "mcp" && 'text-primary border-b-primary',
            )}
            onClick={() => setActiveTab("mcp")}
          >
            MCP Servers
          </button>
          <button
            type="button"
            className={cn(
              'px-md py-sm bg-none border-none border-b-2 border-b-transparent text-text-secondary text-sm font-semibold cursor-pointer transition-all duration-200 hover:text-text',
              activeTab === "voice" && 'text-primary border-b-primary',
            )}
            onClick={() => setActiveTab("voice")}
          >
            Voice
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
            defaultModelInput={defaultModelInput}
            setDefaultModelInput={setDefaultModelInput}
            models={models}
            loadingModels={loadingModels}
            isAdvancedOpen={isAdvancedOpen}
            setIsAdvancedOpen={setIsAdvancedOpen}
            maxToolIterationsInput={maxToolIterationsInput}
            setMaxToolIterationsInput={setMaxToolIterationsInput}
            maxStagnationStepsInput={maxStagnationStepsInput}
            setMaxStagnationStepsInput={setMaxStagnationStepsInput}
            titlePromptInput={titlePromptInput}
            setTitlePromptInput={setTitlePromptInput}
            inferenceDefaultsInput={inferenceDefaultsInput}
            setInferenceDefaultsInput={setInferenceDefaultsInput}
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
        {/* Voice Settings Tab */}
        {activeTab === "voice" && (
          <VoiceSettings onClose={onClose} />
        )}
      </Modal>
    </>
  );
};

export default SettingsModal;
