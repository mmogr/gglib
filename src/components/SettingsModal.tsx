import { FC, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { appLogger } from '../services/platform';
import { useModelsDirectory } from "../hooks/useModelsDirectory";
import { useSettings } from "../hooks/useSettings";
import { useMcpServers } from "../hooks/useMcpServers";
import { useModels } from "../hooks/useModels";
import { UpdateSettingsRequest, InferenceConfig } from "../types";
import { McpServersPanel } from "./McpServersPanel";
import { AddMcpServerModal } from "./AddMcpServerModal";
import { GeneralSettings } from "./SettingsModal/GeneralSettings";
import { InferenceProfiles } from "./SettingsModal/InferenceProfiles";
import { SystemSettings } from "./SettingsModal/SystemSettings";
import { useDesktopSettings } from "./SettingsModal/useDesktopSettings";
import { useNetworkSettings } from './SettingsModal/useNetworkSettings';
import { useAgentGuardSettings } from './SettingsModal/useAgentGuardSettings';
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { Tabs, type TabItem } from "./ui/Tabs";
import type { McpServerInfo } from '../services/transport';

type SettingsTab = "general" | "profiles" | "mcp" | "system";

const SETTINGS_TABS: TabItem<SettingsTab>[] = [
  { id: "general", label: "General" },
  { id: "profiles", label: "Inference Profiles" },
  { id: "mcp", label: "MCP Servers" },
  { id: "system", label: "System" },
];

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const sourceLabels: Record<string, string> = {
  explicit: "Custom path (CLI/UI override)",
  environment: "Configured via .env",
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
  const [proxyApiKeyInput, setProxyApiKeyInput] = useState("");
  const [titlePromptInput, setTitlePromptInput] = useState("");
  const [maxToolIterationsInput, setMaxToolIterationsInput] = useState("");
  const [showFitIndicators, setShowFitIndicators] = useState(true);
  const [trustClientSampling, setTrustClientSampling] = useState(false);
  const [proxyLoopDetection, setProxyLoopDetection] = useState(true);
  const {
    values: desktopValues,
    setValue: setDesktopSetting,
    reset: resetDesktop,
    updates: desktopUpdates,
  } = useDesktopSettings(settings);
  const network = useNetworkSettings(settings);
  const agentGuards = useAgentGuardSettings(settings);
  const [downloadPathInput, setDownloadPathInput] = useState('');
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
      setProxyApiKeyInput(settings.proxyApiKey || "");
      setDownloadPathInput(settings.defaultDownloadPath || "");
      setTitlePromptInput(settings.titleGenerationPrompt || "");
      setMaxToolIterationsInput(settings.maxToolIterations?.toString() || "");
      setShowFitIndicators(settings.showMemoryFitIndicators !== false);
      setTrustClientSampling(settings.trustClientSampling === true);
      // Inverse polarity to trustClientSampling: unset means enabled.
      setProxyLoopDetection(settings.proxyLoopDetection !== false);
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
          // An emptied field means "turn authentication off", which is a
          // `null` (clear the row) rather than a blank string — the backend
          // rejects a blank key precisely so it cannot mean both.
          proxyApiKey: proxyApiKeyInput.trim() || null,
          titleGenerationPrompt: titlePromptInput.trim() || null,
          maxToolIterations: parseNumericInput(maxToolIterationsInput),
          showMemoryFitIndicators: showFitIndicators,
          defaultModelId: parseNumericInput(defaultModelInput),
          inferenceDefaults: inferenceDefaultsInput,
          trustClientSampling,
          proxyLoopDetection,
          defaultDownloadPath: downloadPathInput.trim() || null,
          ...network.updates,
          ...agentGuards.updates,
          ...desktopUpdates,
        };

        // Check if any updates were made
        const hasUpdates =
          updates.defaultContextSize !== undefined ||
          updates.proxyPort !== undefined ||
          updates.llamaBasePort !== undefined ||
          updates.maxDownloadQueueSize !== undefined ||
          updates.proxyApiKey !== undefined ||
          updates.titleGenerationPrompt !== undefined ||
          updates.maxToolIterations !== undefined ||
          updates.showMemoryFitIndicators !== undefined ||
          updates.defaultModelId !== undefined ||
          updates.inferenceDefaults !== undefined ||
          updates.trustClientSampling !== undefined ||
          updates.proxyLoopDetection !== undefined ||
          updates.defaultDownloadPath !== undefined ||
          updates.bindHost !== undefined ||
          updates.shareLan !== undefined ||
          updates.agenticSampling !== undefined ||
          updates.maxStagnationSteps !== undefined ||
          updates.proxyAutostart !== undefined ||
          updates.closeToTray !== undefined ||
          updates.startAtLogin !== undefined;

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
      proxyApiKeyInput,
      titlePromptInput,
      maxToolIterationsInput,
      showFitIndicators,
      defaultModelInput,
      inferenceDefaultsInput,
      trustClientSampling,
      proxyLoopDetection,
      desktopUpdates,
      downloadPathInput,
      network.updates,
      agentGuards.updates,
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
      setContextSizeInput(settings.defaultContextSize?.toString() ?? "");
      setProxyPortInput(settings.proxyPort?.toString() ?? "");
      setServerPortInput(settings.llamaBasePort?.toString() ?? "");
      setMaxQueueSizeInput(settings.maxDownloadQueueSize?.toString() ?? "");
      setProxyApiKeyInput(settings.proxyApiKey ?? "");
      setTitlePromptInput(""); // Reset to default (empty uses DEFAULT_TITLE_GENERATION_PROMPT)
      setShowFitIndicators(true); // Default is enabled
      setTrustClientSampling(false); // Default is disabled
      setProxyLoopDetection(true); // Default is enabled
      resetDesktop();
      setDownloadPathInput('');
      network.reset();
      agentGuards.reset();
    }
  }, [info, settings, resetDesktop, network, agentGuards]);

  const handleRefresh = useCallback(() => {
    refreshDir();
    refreshSettings();
  }, [refreshDir, refreshSettings]);

  // Settings re-fetch whenever the dialog opens, replacing the old footer
  // "Refresh" button.
  useEffect(() => {
    if (isOpen) {
      handleRefresh();
    }
  }, [isOpen, handleRefresh]);

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
        size="lg"
        height="fixed"
        preventClose={saving}
        subHeader={
          <Tabs
            tabs={SETTINGS_TABS}
            activeId={activeTab}
            onChange={setActiveTab}
            aria-label="Settings sections"
            divider={false}
          />
        }
        footer={
          activeTab === "general" ? (
            <>
              <Button type="button" variant="secondary" onClick={onClose} disabled={saving}>
                Cancel
              </Button>
              <Button type="submit" form="settings-general-form" variant="primary" disabled={saving || loading}>
                {saving ? "Saving…" : "Save changes"}
              </Button>
            </>
          ) : undefined
        }
      >

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
            proxyApiKeyInput={proxyApiKeyInput}
            setProxyApiKeyInput={setProxyApiKeyInput}
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
            titlePromptInput={titlePromptInput}
            setTitlePromptInput={setTitlePromptInput}
            inferenceDefaultsInput={inferenceDefaultsInput}
            setInferenceDefaultsInput={setInferenceDefaultsInput}
            downloadPathInput={downloadPathInput}
            setDownloadPathInput={setDownloadPathInput}
            networkSettings={network.values}
            setNetworkSetting={network.setValue}
            agentGuardSettings={agentGuards.values}
            setAgentGuardSetting={agentGuards.setValue}
            desktopSettings={desktopValues}
            setDesktopSetting={setDesktopSetting}
            trustClientSampling={trustClientSampling}
            setTrustClientSampling={setTrustClientSampling}
            proxyLoopDetection={proxyLoopDetection}
            setProxyLoopDetection={setProxyLoopDetection}
            onSubmit={handleSubmit}
            onReset={handleReset}
            loading={loading}
            saving={saving}
            error={error}
            successMessage={successMessage}
          />
        )}

        {/* Inference Profiles Tab */}
        {activeTab === "profiles" && <InferenceProfiles />}

        {activeTab === "system" && <SystemSettings />}

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
