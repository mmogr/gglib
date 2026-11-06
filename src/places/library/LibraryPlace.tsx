/**
 * The library, and the inspector beside it while it has a subject.
 *
 * The layout route over `/library`, `/models/:id` and `/far/:id`: the
 * selection is the address, so picking a row navigates and the place stays
 * mounted. The main column is the library's rows; until the one search
 * lands (L2), `?view=add` shows the Hugging Face browser and the local-file
 * form in their place, with a way back. The inspector on the right is
 * today's (`ModelInspectorPanel`, `FarModelInspector`, the Hugging Face
 * preview) until its redesign (B2), wired as before: the download queue for
 * its Download buttons, `?serve=1` opening its Serve dialog once it can, and
 * a server it started opening that server's chat. With nothing picked the
 * rows take the width.
 *
 * @module places/library/LibraryPlace
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { ArrowLeft } from 'lucide-react';
import { FarModelInspector, ModelInspectorPanel } from '../../components/ModelInspectorPanel';
import AddDownloadContent from '../../components/ModelLibraryPanel/AddDownloadContent';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import { usePairedModels } from '../../hooks/usePairedModels';
import { useServers, type ServerViewModel } from '../../hooks/useServers';
import { HANDED, setChatHandoff } from '../../app/chatHandoff';
import { inspectorSubject, libraryIntent, paths, settingsAt } from '../../app/paths';
import { useDownloads } from '../../services/downloadsRegistry';
import { refreshModels, removeModel as removeLibraryModel, updateModel } from '../../services/modelEvents';
import { useModel } from '../../services/modelRegistry';
import { getTransport } from '../../services/transport';
import type { HfModelSummary } from '../../types';
import type { ModelRef } from '../../types/generated/ModelRef';
import { formatError } from '../../utils/errors';
import { Downloads } from './DownloadRow';
import { EngineBanner } from './EngineBanner';
import { DEFAULT_SORT, type LibrarySort } from './groups';
import { LibraryHeader } from './LibraryHeader';
import { LibraryRows } from './LibraryRows';

export default function LibraryPlace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const { servers, stopServer } = useServers();
  const { snapshot: downloadQueue } = useDownloads();
  const paired = usePairedModels();
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<LibrarySort>(DEFAULT_SORT);
  const [selectedHfModel, setSelectedHfModel] = useState<HfModelSummary | null>(null);
  // The inspector's Serve dialog opener, state so an intent can wait for it.
  const [openServeModal, setOpenServeModal] = useState<(() => void) | null>(null);

  const subject = inspectorSubject(location);
  const selectedModelId = subject?.kind === 'model' ? subject.id : null;
  const selectedModel = useModel(selectedModelId);
  // A far pick holds only while the paired rows are that machine's.
  const farPick: ModelRef | null = useMemo(
    () => (subject?.kind === 'far' && paired.group ? { machine: paired.group.machine, id: subject.id } : null),
    [subject?.kind, subject?.id, paired.group],
  );
  const intent = libraryIntent(location.search);

  // The Add pane is an address; leaving it drops its Hugging Face pick.
  useEffect(() => {
    if (intent.add === null) setSelectedHfModel(null);
  }, [intent.add]);

  // `?serve=1` opens the picked model's Serve dialog once, when the inspector
  // has an opener (a cold load waits for it), then leaves the address.
  useEffect(() => {
    if (!intent.serve) return;
    if (openServeModal === null && selectedModelId !== null) return;
    openServeModal?.();
    void navigate(location.pathname, { replace: true });
  }, [intent.serve, openServeModal, selectedModelId, location.pathname, navigate]);

  const pickLocal = useCallback((id: number) => navigate(paths.model(id)), [navigate]);
  const pickFar = useCallback((id: number) => navigate(paths.far(id)), [navigate]);
  const removeModel = useCallback(
    async (id: number) => {
      await removeLibraryModel(id);
      if (selectedModelId === id) navigate(paths.library());
    },
    [selectedModelId, navigate],
  );
  const unload = useCallback(
    (id: number) => void stopServer(id).catch((err) => showToast(formatError(err), 'error')),
    [stopServer, showToast],
  );

  // A server started from the inspector opens its chat; the port is handed
  // on, since the registry may not have heard of it yet.
  const handleServerStarted = async (server: ServerViewModel) => {
    setChatHandoff({ modelId: server.modelId, port: server.port, modelName: server.modelName });
    navigate(paths.chat(server.modelId), { state: HANDED });
  };

  const inspecting = selectedModelId !== null || farPick !== null || selectedHfModel !== null;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <section aria-label="Library" className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <LibraryHeader sort={sort} onSort={setSort} query={intent.add === null ? query : null} onQuery={setQuery} />
        {intent.add !== null ? (
          <div className="flex min-h-0 flex-1 flex-col">
            <Downloads query="" />
            <div className="px-md pb-sm">
              <Button variant="ghost" size="sm" leftIcon={<Icon icon={ArrowLeft} size={14} />} onClick={() => navigate(paths.library())}>
                Your models
              </Button>
            </div>
            <div className="min-h-0 flex-1">
              <AddDownloadContent
                onModelAdded={refreshModels}
                activeSubTab={intent.add}
                onSubTabChange={(sub) => {
                  if (sub !== 'browse') setSelectedHfModel(null);
                  navigate(paths.libraryAdd(sub), { replace: true });
                }}
                onSelectHfModel={setSelectedHfModel}
                selectedHfModelId={selectedHfModel?.id}
              />
            </div>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto pb-md">
            <EngineBanner />
            <LibraryRows
              query={query}
              sort={sort}
              subject={subject}
              servers={servers}
              paired={paired}
              onPickLocal={pickLocal}
              onPickFar={pickFar}
              onUnload={unload}
            />
          </div>
        )}
      </section>

      {inspecting && (
        <aside aria-label="Inspector" className="flex w-[380px] shrink-0 flex-col overflow-y-auto bg-surface">
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
              onAddTag={(id, tag) => getTransport().addModelTag(id, tag)}
              onRemoveTag={(id, tag) => getTransport().removeModelTag(id, tag)}
              getModelDetail={(id) => getTransport().getModelDetail(id)}
              onRefresh={refreshModels}
              downloadQueue={downloadQueue}
              onRegisterServeModalOpener={(opener) => setOpenServeModal(() => opener)}
              onBenchmark={(modelId) => navigate(paths.benchmarks([modelId]))}
              onOpenSystemSettings={() => navigate(settingsAt(location, 'system'), { replace: true })}
            />
          )}
        </aside>
      )}
    </div>
  );
}
