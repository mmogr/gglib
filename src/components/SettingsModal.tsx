import { FC, FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { appLogger } from '../services/platform';
import { useModelsDirectory } from "../hooks/useModelsDirectory";
import { useSettings } from "../hooks/useSettings";
import { useMcpServers } from "../hooks/useMcpServers";
import { useModels } from "../hooks/useModels";
import { UpdateSettingsRequest, SparseInferenceConfig, type AppSettings } from "../types";
import { McpServersPanel } from "./McpServersPanel";
import { AddMcpServerModal } from "./AddMcpServerModal";
import { GeneralSettings } from "./SettingsModal/GeneralSettings";
import { InferenceProfiles } from "./SettingsModal/InferenceProfiles";
import { SystemSettings } from "./SettingsModal/SystemSettings";
import { useDesktopSettings } from "./SettingsModal/useDesktopSettings";
import { useNetworkSettings } from './SettingsModal/useNetworkSettings';
import { useAgentGuardSettings } from './SettingsModal/useAgentGuardSettings';
import { changedFields, generalInputs, generalRequest } from './SettingsModal/settingsRequest';
import { Modal } from "./ui/Modal";
import { Button } from "./ui/Button";
import { Tabs, type TabItem } from "./ui/Tabs";
import type { McpServerInfo } from '../services/transport';

export type SettingsTab = "general" | "profiles" | "mcp" | "system";

const SETTINGS_TABS: TabItem<SettingsTab>[] = [
  { id: "general", label: "General" },
  { id: "profiles", label: "Inference Profiles" },
  { id: "mcp", label: "MCP Servers" },
  { id: "system", label: "System" },
];

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  /** The tab it opens on; General when absent. */
  initialTab?: SettingsTab;
}

/** What the form compares against before any settings have loaded. */
const NO_SETTINGS = {} as AppSettings;

const sourceLabels: Record<string, string> = {
  explicit: "Custom path (CLI/UI override)",
  environment: "Configured via .env",
  default: "Default (~/.local/share/llama_models)",
};

export const SettingsModal: FC<SettingsModalProps> = ({ isOpen, onClose, initialTab }) => {
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
  const {
    values: desktopValues,
    setValue: setDesktopSetting,
    updates: desktopUpdates,
  } = useDesktopSettings(settings);
  const network = useNetworkSettings(settings);
  const agentGuards = useAgentGuardSettings(settings);
  const [downloadPathInput, setDownloadPathInput] = useState('');
  const [defaultModelInput, setDefaultModelInput] = useState("");
  const [inferenceDefaultsInput, setInferenceDefaultsInput] = useState<SparseInferenceConfig | undefined>(undefined);
  const [isAdvancedOpen, setIsAdvancedOpen] = useState(false);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab ?? "general");
  
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
      const loaded = generalInputs(settings);
      setContextSizeInput(loaded.contextSize);
      setProxyPortInput(loaded.proxyPort);
      setServerPortInput(loaded.serverPort);
      setMaxQueueSizeInput(loaded.maxQueueSize);
      setProxyApiKeyInput(loaded.proxyApiKey);
      setDownloadPathInput(loaded.downloadPath);
      setTitlePromptInput(loaded.titlePrompt);
      setMaxToolIterationsInput(loaded.maxToolIterations);
      setShowFitIndicators(loaded.showFitIndicators);
      setTrustClientSampling(loaded.trustClientSampling);
      setDefaultModelInput(loaded.defaultModel);
      setInferenceDefaultsInput(loaded.inferenceDefaults);
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

        // Only what the person changed: a field left alone must not put back
        // a value written elsewhere while the dialog was open (#1059).
        const onScreen = generalRequest({
          contextSize: contextSizeInput,
          proxyPort: proxyPortInput,
          serverPort: serverPortInput,
          maxQueueSize: maxQueueSizeInput,
          proxyApiKey: proxyApiKeyInput,
          downloadPath: downloadPathInput,
          titlePrompt: titlePromptInput,
          maxToolIterations: maxToolIterationsInput,
          showFitIndicators,
          trustClientSampling,
          defaultModel: defaultModelInput,
          inferenceDefaults: inferenceDefaultsInput,
        });
        const updates: UpdateSettingsRequest = {
          ...changedFields(onScreen, generalRequest(generalInputs(settings ?? NO_SETTINGS))),
          ...network.updates,
          ...agentGuards.updates,
          ...desktopUpdates,
        };

        if (Object.keys(updates).length > 0) {
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
      desktopUpdates,
      downloadPathInput,
      network.updates,
      agentGuards.updates,
      info,
      settings,
      saveDir,
      saveSettings,
    ]
  );

  // Scoped to the one field this sits beside.
  //
  // It used to reset thirteen other settings as well — the title prompt, fit
  // indicators, client-sampling trust, proxy loop detection, the download
  // path, bind host and LAN sharing, the three desktop toggles and the three
  // agent guards — from a link in the Models Directory field's action slot,
  // and `handleSubmit` then sent all of them. It had never run: the gate read
  // `info?.defaultPath`, a key the wire has never carried, so the button did
  // not render. Correcting that spelling is what would have made a
  // thirteen-setting revert reachable from a control labelled for one field,
  // four of them behind a collapsed disclosure the user never opened.
  const handleReset = useCallback(() => {
    if (info?.default_path) {
      setPathInput(info.default_path);
    }
  }, [info]);

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
