import { useEffect, useMemo, useState } from 'react';
import { Database, Download, Globe, MessageSquare, Settings } from 'lucide-react';

import { ChatInterface } from './components/ChatInterface';
import { DownloadsDialog } from './components/DownloadsDialog';
import { HuggingFaceDialog } from './components/HuggingFaceDialog';
import { ModelSelector } from './components/ModelSelector';
import { ModelsDialog } from './components/ModelsDialog';
import { SettingsDialog } from './components/SettingsDialog';
import { Button } from './components/ui/button';
import { Toaster } from './components/ui/sonner';
import { api } from './services/api';
import { subscribeToEvent } from '../services/clients/events';
import type { Model, ServerStatus } from './types/api';

export default function App() {
  const [models, setModels] = useState<Model[]>([]);
  const [servers, setServers] = useState<ServerStatus[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [modelsOpen, setModelsOpen] = useState(false);
  const [downloadsOpen, setDownloadsOpen] = useState(false);
  const [hfOpen, setHfOpen] = useState(false);

  const selectedModel = useMemo(
    () => models.find((m) => (m.id ?? null) === selectedModelId) ?? null,
    [models, selectedModelId]
  );

  useEffect(() => {
    void loadModels();
    void loadServers();

    const unsubServer = subscribeToEvent('server', () => {
      void loadServers();
    });
    const unsubDownload = subscribeToEvent('download', () => {
      // Download status surfaces in dialogs; reload when opened.
    });

    return () => {
      unsubServer();
      unsubDownload();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const loadModels = async () => {
    const data = await api.getModels();
    setModels(data);
    if (data.length > 0 && selectedModelId === null) {
      setSelectedModelId(data[0]?.id ?? null);
    }
  };

  const loadServers = async () => {
    const data = await api.getServers();
    setServers(data);
  };

  const getServerStatus = (modelId: number | null): ServerStatus | undefined => {
    if (modelId === null) return undefined;
    return servers.find((s) => s.model_id === modelId);
  };

  return (
    <div className="flex h-screen bg-background text-foreground">
      <div className="w-64 border-r border-border bg-card flex flex-col">
        <div className="p-4 border-b border-border">
          <h1 className="font-semibold text-lg flex items-center gap-2">
            <MessageSquare className="size-5" />
            GGLib
          </h1>
        </div>

        <div className="p-4 border-b border-border">
          <ModelSelector
            models={models}
            selectedModelId={selectedModelId}
            onSelectModelId={setSelectedModelId}
            serverStatus={getServerStatus(selectedModelId)}
            onRefresh={async () => {
              await loadModels();
              await loadServers();
            }}
          />
        </div>

        <div className="flex-1 p-2 space-y-1">
          <Button variant="ghost" className="w-full justify-start gap-2" onClick={() => setModelsOpen(true)}>
            <Database className="size-4" />
            Manage Models
          </Button>
          <Button variant="ghost" className="w-full justify-start gap-2" onClick={() => setDownloadsOpen(true)}>
            <Download className="size-4" />
            Downloads
          </Button>
          <Button variant="ghost" className="w-full justify-start gap-2" onClick={() => setHfOpen(true)}>
            <Globe className="size-4" />
            HuggingFace Hub
          </Button>
        </div>

        <div className="p-2 border-t border-border">
          <Button variant="ghost" className="w-full justify-start gap-2" onClick={() => setSettingsOpen(true)}>
            <Settings className="size-4" />
            Settings
          </Button>
        </div>
      </div>

      <div className="flex-1 flex flex-col">
        {selectedModel ? (
          <ChatInterface
            model={selectedModel}
            serverStatus={getServerStatus(selectedModel.id ?? null)}
            onServerChange={loadServers}
          />
        ) : (
          <div className="flex-1 flex items-center justify-center text-muted-foreground">
            <div className="text-center space-y-4">
              <MessageSquare className="size-16 mx-auto opacity-20" />
              <div>
                <p className="text-lg font-medium">No model selected</p>
                <p className="text-sm">Add a model to get started</p>
              </div>
              <Button onClick={() => setModelsOpen(true)}>Manage Models</Button>
            </div>
          </div>
        )}
      </div>

      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      <ModelsDialog open={modelsOpen} onOpenChange={setModelsOpen} models={models} onModelsChange={loadModels} />
      <DownloadsDialog open={downloadsOpen} onOpenChange={setDownloadsOpen} />
      <HuggingFaceDialog open={hfOpen} onOpenChange={setHfOpen} />

      <Toaster />
    </div>
  );
}
