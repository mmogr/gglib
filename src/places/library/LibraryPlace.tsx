/**
 * The library, and the inspector beside it while it has a subject.
 *
 * The layout route over `/library`, `/models/:id` (and its `/log`),
 * `/far/:id` and `/get/:owner/:name`: the selection is the address, so
 * picking a row navigates and the place stays mounted. The main column is
 * the library's rows, then what the one search found on Hugging Face
 * (`hf/`). The inspector (`inspector/Inspector`) holds the one subject the
 * address names: a model of this machine's or its log, the paired
 * machine's model, or a Hugging Face repo; a repo picked here hands its
 * summary to the address it opens, so it is not read twice.
 * The inspector reads `?serve=1` itself (its Load with options sheet), and
 * a server it started for a chat opens that chat. `?add=browse` focuses the
 * search; `?add=file` opens the native picker on the desktop and the typed
 * path on the web. With nothing picked the rows take the width.
 *
 * @module places/library/LibraryPlace
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { Inspector } from '../../inspector/Inspector';
import { BESIDE_INSPECTOR } from '../../inspector/useInspectorWidth';
import { cn } from '../../utils/cn';
import { Button } from '../../components/ui/Button';
import { useToastContext } from '../../contexts/ToastContext';
import { usePairedModels } from '../../hooks/usePairedModels';
import { useServers, type ServerViewModel } from '../../hooks/useServers';
import { openChatOn } from '../../app/openChat';
import { inspectorSubject, libraryIntent, paths } from '../../app/paths';
import { useDownloads } from '../../services/downloadsRegistry';
import type { HfPickState } from '../../inspector/hf/HfInspector';
import { removeModel as removeLibraryModel, updateModel } from '../../services/modelEvents';
import { isDesktop, pickGgufFile } from '../../services/platform';
import type { HfModelSummary } from '../../types';
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
  // The add-a-file row: closed, open to type a path, or the picked path that failed.
  const [adding, setAdding] = useState<{ path: string; error: string } | 'typed' | null>(null);
  const field = useRef<HTMLInputElement>(null);

  const subject = inspectorSubject(location);
  const selectedModelId = subject?.kind === 'model' ? subject.id : null;
  const intent = libraryIntent(location.search);
  const subjectKey = subject && (subject.kind === 'hf' ? `hf:${subject.repo}` : `${subject.kind}:${subject.id}`);

  const pickHf = useCallback(
    (model: HfModelSummary, replace = false) => navigate(paths.hf(model.id), { replace, state: { hf: model } satisfies HfPickState }),
    [navigate],
  );
  const search = useLibrarySearch(pickHf);
  const { cancelOpen } = search;

  // A new subject drops a repo still being read.
  useEffect(() => {
    if (subjectKey !== null) cancelOpen();
  }, [subjectKey, cancelOpen]);

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

  // `replace`: a pick made with the arrow keys from the row picked before (`Listbox`).
  const pickLocal = useCallback((id: number, replace = false) => navigate(paths.model(id), { replace }), [navigate]);
  const pickFar = useCallback((id: number, replace = false) => navigate(paths.far(id), { replace }), [navigate]);
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

  // A server started from the inspector for a chat opens that chat; the
  // port is handed on, since the registry may not have heard of it yet.
  const handleServerStarted = (server: ServerViewModel) => {
    void openChatOn(navigate, { id: server.modelId, port: server.port, name: server.modelName });
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
      <section aria-label="Library" className={cn('flex flex-1 flex-col overflow-hidden', BESIDE_INSPECTOR)}>
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
          <HfGroup search={search} pickedId={subject?.kind === 'hf' ? subject.repo : null} onPick={pickHf} queueFull={downloadQueue?.full ?? false} />
        </div>
      </section>

      <Inspector
        paired={paired}
        servers={servers}
        onServerStarted={handleServerStarted}
        onStopServer={stopServer}
        onRemoveModel={removeModel}
        onUpdateModel={updateModel}
        onOpenSystemSettings={() => navigate(paths.settings('engines'))}
      />
    </div>
  );
}
