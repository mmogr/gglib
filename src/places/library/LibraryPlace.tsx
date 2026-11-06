/**
 * The library, and the inspector beside it while it has a subject.
 *
 * The layout route over `/library`, `/models/:id` and `/far/:id`: the
 * selection is the address, so picking a row navigates and the place stays
 * mounted. The main column is the library's rows, then what the one search
 * found on Hugging Face (`hf/`). The inspector holds one subject: a model
 * of this machine's or the paired one's (by address), or a Hugging Face
 * repo (picked here; any address with a subject drops it). It is today's
 * (`ModelInspectorPanel`, `FarModelInspector`, the Hugging Face preview)
 * until its redesign (B2), wired as before: the download queue for its
 * Download buttons, `?serve=1` opening its Serve dialog once it can, and a
 * server it started opening that server's chat. `?add=browse` focuses the
 * search; `?add=file` opens the native picker on the desktop and the typed
 * path on the web. With nothing picked the rows take the width.
 *
 * @module places/library/LibraryPlace
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { FarModelInspector, ModelInspectorPanel } from '../../components/ModelInspectorPanel';
import { Button } from '../../components/ui/Button';
import { useToastContext } from '../../contexts/ToastContext';
import { usePairedModels } from '../../hooks/usePairedModels';
import { useServers, type ServerViewModel } from '../../hooks/useServers';
import { HANDED, setChatHandoff } from '../../app/chatHandoff';
import { inspectorSubject, libraryIntent, paths, settingsAt } from '../../app/paths';
import { useDownloads } from '../../services/downloadsRegistry';
import { refreshModels, removeModel as removeLibraryModel, updateModel } from '../../services/modelEvents';
import { useModel } from '../../services/modelRegistry';
import { isDesktop, pickGgufFile } from '../../services/platform';
import { getTransport } from '../../services/transport';
import type { HfModelSummary } from '../../types';
import type { ModelRef } from '../../types/generated/ModelRef';
import { formatError } from '../../utils/errors';
import { AddFileRow, addGgufFile } from './AddFileRow';
import { EngineBanner } from './EngineBanner';
import { DEFAULT_SORT, type LibrarySort } from './groups';
import { LibraryHeader } from './LibraryHeader';
import { LibraryRows } from './LibraryRows';
import { HfGroup } from './hf/HfGroup';
import { useLibrarySearch } from './hf/useLibrarySearch';

export default function LibraryPlace() {
  const location = useLocation();
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const { servers, stopServer } = useServers();
  const { snapshot: downloadQueue } = useDownloads();
  const paired = usePairedModels();
  const [sort, setSort] = useState<LibrarySort>(DEFAULT_SORT);
  const [selectedHfModel, setSelectedHfModel] = useState<HfModelSummary | null>(null);
  // The add-a-file row: closed, open to type a path, or the picked path that failed.
  const [adding, setAdding] = useState<{ path: string; error: string } | 'typed' | null>(null);
  const field = useRef<HTMLInputElement>(null);
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
  const subjectKey = subject && `${subject.kind}:${subject.id}`;

  // An address with a subject is the inspector's one subject.
  useEffect(() => {
    if (subjectKey !== null) setSelectedHfModel(null);
  }, [subjectKey]);

  const pickHf = useCallback(
    (model: HfModelSummary) => {
      if (subjectKey !== null) navigate(paths.library());
      setSelectedHfModel(model);
    },
    [subjectKey, navigate],
  );
  const search = useLibrarySearch(pickHf);

  // `?add=` acts once, then leaves the address.
  const handled = useRef<string | null>(null);
  useEffect(() => {
    if (intent.add === null || handled.current === location.key) return;
    handled.current = location.key;
    void navigate(location.pathname, { replace: true });
    if (intent.add === 'browse') field.current?.focus();
    else if (!isDesktop()) setAdding('typed');
    else
      void pickGgufFile()
        .then(async ({ path }) => {
          if (path) await addGgufFile(path).catch((err) => setAdding({ path, error: formatError(err) }));
        })
        .catch(() => setAdding('typed'));
  }, [intent.add, location.key, location.pathname, navigate]);

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
        <LibraryHeader
          sort={sort}
          onSort={setSort}
          query={search.query}
          onQuery={search.setQuery}
          onEnter={search.enter}
          onFocus={search.onFocus}
          inputRef={field}
        />
        <div className="min-h-0 flex-1 overflow-y-auto pb-md">
          {adding !== null && (
            <AddFileRow
              key={typeof adding === 'string' ? adding : adding.path}
              initial={typeof adding === 'string' ? undefined : adding}
              onClose={() => setAdding(null)}
            />
          )}
          <EngineBanner />
          <LibraryRows
            query={search.query}
            sort={sort}
            subject={subject}
            servers={servers}
            paired={paired}
            onPickLocal={pickLocal}
            onPickFar={pickFar}
            onUnload={unload}
          />
          {search.query.trim() !== '' && (
            <Button variant="ghost" size="sm" className="mx-md" onClick={search.enter}>
              {`Search Hugging Face for '${search.query.trim()}' ↵`}
            </Button>
          )}
          <HfGroup search={search} pickedId={selectedHfModel?.id ?? null} onPick={pickHf} queueFull={downloadQueue?.full ?? false} />
        </div>
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
