import { useState, useCallback, useEffect, useMemo } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { useModel, useModelLibrary } from '../services/modelRegistry';
import { onModelLibraryChange, refreshLibrary, removeModel as removeLibraryModel, updateModel } from '../services/modelEvents';
import { usePairedModels } from '../hooks/usePairedModels';
import { useServers } from '../hooks/useServers';
import { useTags } from '../hooks/useTags';
import { useDownloadManager } from '../hooks/useDownloadManager';
import { useDownloadCompletionEffects } from '../hooks/useDownloadCompletionEffects';
import { useModelFilterOptions } from '../hooks/useModelFilterOptions';
import ModelLibraryPanel from '../components/ModelLibraryPanel/ModelLibraryPanel';
import { FarModelInspector, ModelInspectorPanel } from '../components/ModelInspectorPanel';
import { GlobalDownloadStatus } from '../components/GlobalDownloadStatus';
import TwoPanelLayout from '../components/TwoPanelLayout';
import { HANDED, setChatHandoff } from '../app/chatHandoff';
import { inspectorSubject, libraryIntent, paths, settingsAt } from '../app/paths';
import { useMccFilters } from './modelControlCenter/useMccFilters';
import { useMccLayout } from './modelControlCenter/useMccLayout';
import { HfModelSummary } from '../types';
import type { ModelRef } from '../types/generated/ModelRef';
import type { ServerViewModel } from '../hooks/useServers';
import { SidebarTabId } from '../components/ModelLibraryPanel/ModelLibraryPanel';
import { AddDownloadSubTab } from '../components/ModelLibraryPanel/AddDownloadContent';
import { getTransport } from '../services/transport';

/**
 * The library and the inspector beside it. The selection is the address:
 * `/library` nothing, `/models/:id` one of this machine's models, `/far/:id`
 * one of the paired machine's; a chat, the benchmarks and the settings are
 * addresses too, so this page shows one screen and moves to the others.
 */
export default function ModelControlCenterPage() {
  const location = useLocation();
  const navigate = useNavigate();
  const { loading, error } = useModelLibrary();
  const { servers, stopServer } = useServers();
  // The paired machine's rows, and the one selection the address names.
  const paired = usePairedModels();
  const subject = inspectorSubject(location);
  const selectedModelId = subject?.kind === 'model' ? subject.id : null;
  const selectedModel = useModel(selectedModelId);
  // A far pick holds only while the paired rows are that machine's.
  const farPick: ModelRef | null = useMemo(
    () => (subject?.kind === 'far' && paired.group ? { machine: paired.group.machine, id: subject.id } : null),
    [subject?.kind, subject?.id, paired.group],
  );
  const pickLocal = useCallback(
    (id: number | null) => navigate(id === null ? paths.library() : paths.model(id)),
    [navigate],
  );
  const pickFar = useCallback((model: ModelRef) => navigate(paths.far(model.id)), [navigate]);
  const removeModel = useCallback(async (id: number) => {
    await removeLibraryModel(id);
    if (selectedModelId === id) navigate(paths.library());
  }, [selectedModelId, navigate]);
  const { tags, loadTags, addTagToModel, removeTagFromModel } = useTags();
  const { filterOptions, refresh: refreshFilterOptions } = useModelFilterOptions();

  // Another client's edits, and Refresh, move more than the list, which the
  // registry re-reads itself: a new architecture or tag has to reach the
  // filter options too.
  const refreshFilterChoices = useCallback(() => void Promise.all([refreshFilterOptions(), loadTags()]), [refreshFilterOptions, loadTags]);
  useEffect(() => onModelLibraryChange(refreshFilterChoices), [refreshFilterChoices]);

  // Download ending effects - batches completions, triggers refresh, shows toasts
  const { onCompleted, onFailed } = useDownloadCompletionEffects({
    refreshModels: refreshLibrary,
  });
  
  // The download queue - lifted to page level so it's always visible
  const {
    snapshot: downloadQueue,
    cancellingId,
    lastQueueSummary,
    cancel: cancelDownload,
    refreshQueue,
    clearQueueSummary,
  } = useDownloadManager({
    onCompleted,
    onFailed,
  });

  // Track whether user dismissed completion banner
  const [downloadDismissed] = useState(false);
  
  // Sidebar tab state (for the new tabbed sidebar)
  const [sidebarTab, setSidebarTab] = useState<SidebarTabId>('models');
  
  // HuggingFace model selection state (for preview in inspector)
  const [selectedHfModel, setSelectedHfModel] = useState<HfModelSummary | null>(null);

  // The inspector's Serve dialog opener, state so an intent can wait for it.
  const [openServeModal, setOpenServeModal] = useState<(() => void) | null>(null);
  
  // Panel width state (percentages) - now just two columns
  const { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize } = useMccLayout();

  const {
    searchQuery,
    setSearchQuery,
    filters,
    onFiltersChange: handleFiltersChange,
    onClearFilters: handleClearFilters,
    filteredModels,
    activeSubTab,
    setActiveSubTab,
    handleModelAdded,
  } = useMccFilters({ refreshFilterOptions, loadTags });

  // What the native menu asked for, carried in the address and acted on once:
  // the Add pane on a tab, or the picked model's Serve dialog, which waits
  // until the inspector has an opener (a cold load of `?serve=1`).
  const intent = libraryIntent(location.search);
  useEffect(() => {
    if (intent.add === null && !intent.serve) return;
    if (intent.serve && openServeModal === null && selectedModelId !== null) return;
    if (intent.add !== null) {
      setSidebarTab('add');
      setActiveSubTab(intent.add);
    }
    if (intent.serve) openServeModal?.();
    void navigate(location.pathname, { replace: true });
  }, [intent.add, intent.serve, openServeModal, selectedModelId, location.pathname, navigate, setActiveSubTab]);

  // Handler for selecting a local model (clears HF selection)
  const handleSelectLocalModel = (id: number | null) => {
    pickLocal(id);
    if (id !== null) {
      setSelectedHfModel(null); // Clear HF selection when selecting local model
    }
  };

  // Handler for selecting an HF model for preview (clears local selection)
  const handleSelectHfModel = (model: HfModelSummary | null) => {
    setSelectedHfModel(model);
    if (model !== null) {
      pickLocal(null); // Clear the library's selection when selecting HF model
    }
  };

  // Handler for sidebar tab changes - manages model selection based on context
  const handleSidebarTabChange = (tab: SidebarTabId) => {
    setSidebarTab(tab);
    // Clear appropriate model selection based on tab context
    if (tab === 'add') {
      // Clear the library's selection when entering Add Models tab
      pickLocal(null);
    } else {
      // Clear HF model selection when leaving the Add Models tab
      setSelectedHfModel(null);
    }
  };

  // Handler for subtab changes within Add Models - clears HF selection when leaving Browse HF
  const handleSubTabChange = (subtab: AddDownloadSubTab) => {
    setActiveSubTab(subtab);
    // Clear HF model selection when switching away from Browse HF subtab
    if (subtab !== 'browse') {
      setSelectedHfModel(null);
    }
  };

  // A server started from the inspector opens its chat; the port is handed
  // on, since the registry may not have heard of it yet.
  const handleServerStarted = async (serverInfo: ServerViewModel) => {
    setChatHandoff({ modelId: serverInfo.modelId, port: serverInfo.port, modelName: serverInfo.modelName });
    navigate(paths.chat(serverInfo.modelId), { state: HANDED });
  };

  return (
    <div className="flex flex-col w-full min-h-full overflow-auto md:h-full md:min-h-0 md:overflow-hidden">
      <TwoPanelLayout
        ref={layoutRef}
        className="overflow-visible"
        leftWidth={leftPanelWidth}
        onResizeStart={handlePointerDown}
        onKeyboardResize={handleKeyboardResize}
        left={
          <ModelLibraryPanel
            models={filteredModels}
            selectedModelId={selectedModelId}
            onSelectModel={handleSelectLocalModel}
            loading={loading}
            error={error}
            onRefresh={refreshLibrary}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            tags={tags}
            servers={servers}
            filterOptions={filterOptions}
            filters={filters}
            onFiltersChange={handleFiltersChange}
            onClearFilters={handleClearFilters}
            onModelAdded={handleModelAdded}
            activeSubTab={activeSubTab}
            onSubTabChange={handleSubTabChange}
            onSelectHfModel={handleSelectHfModel}
            selectedHfModelId={selectedHfModel?.id}
            activeTab={sidebarTab}
            onTabChange={handleSidebarTabChange}
            paired={paired}
            farPick={farPick}
            onPickFar={(model) => { pickFar(model); setSelectedHfModel(null); }}
          />
        }
        rightClassName="gap-0"
        right={
          <>
            {!downloadDismissed && (
              <GlobalDownloadStatus
                snapshot={downloadQueue}
                cancellingId={cancellingId}
                lastQueueSummary={lastQueueSummary}
                onCancel={cancelDownload}
                onDismissSummary={clearQueueSummary}
                onRefreshQueue={refreshQueue}
              />
            )}
            {farPick ? (
              <FarModelInspector model={farPick} paired={paired} onChat={(far) => navigate(paths.farChat(far.id))} />
            ) : (
              <ModelInspectorPanel
                model={selectedModel}
                selectedHfModel={selectedHfModel}
                onServerStarted={handleServerStarted}
                onOpenChat={(modelId) => navigate(paths.chat(modelId))}
                onStopServer={stopServer}
                servers={servers}
                onRemoveModel={removeModel}
                onUpdateModel={updateModel}
                onAddTag={addTagToModel}
                onRemoveTag={removeTagFromModel}
                getModelDetail={(id) => getTransport().getModelDetail(id)}
                onRefresh={refreshLibrary}
                downloadQueue={downloadQueue}
                onRegisterServeModalOpener={(opener) => setOpenServeModal(() => opener)}
                onBenchmark={(modelId) => navigate(paths.benchmarks([modelId]))}
                onOpenSystemSettings={() => navigate(settingsAt(location, 'system'), { replace: true })}
              />
            )}
          </>
        }
      />
    </div>
  );
}
