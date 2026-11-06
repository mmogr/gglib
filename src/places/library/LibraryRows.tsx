/**
 * The library's groups, in order: Downloading, Loaded, On this Mac, then the
 * paired machine's. A model is in one of them and moves when its state does
 * (`groups.ts`). Each group of models is a listbox; its rows are options.
 *
 * Every word on a row is the daemon's or arranged from its facts: the state
 * beside a Loaded row is `RESIDENCY_WORDS` for the launch state the server
 * list gives (`Loaded` for a server announced before the list is read again,
 * since it is announced once it answers), the paired machine's reach is
 * `REACH_WORDS` for the remote status's `reach`, and a download is said in
 * its row's own text.
 *
 * @module places/library/LibraryRows
 */

import type { FC, MouseEvent, ReactNode } from 'react';
import { CopyValue } from '../../components/CopyValue';
import { RESIDENCY_MARK, REACH_MARK, StatusMark } from '../../components/status/StatusMark';
import { REACH_WORDS, RESIDENCY_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { useToastContext } from '../../contexts/ToastContext';
import type { PairedModelsState } from '../../hooks/usePairedModels';
import type { ServerViewModel } from '../../hooks/useServers';
import { useDownloads } from '../../services/downloadsRegistry';
import { cancelDownload, removeFromQueue } from '../../services/downloadsEvents';
import { refreshLibrary } from '../../services/modelEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import { useRemoteState } from '../../services/remoteRegistry';
import type { GgufModel } from '../../types';
import type { InspectorSubject } from '../../app/paths';
import { canSee } from '../../utils/canSee';
import { formatParamCount, formatSize } from '../../utils/format';
import { imageFamilyLabel } from '../../utils/imageFamily';
import { DownloadRow } from './DownloadRow';
import { libraryGroups, matchesDownload, matchesFar, type LibrarySort } from './groups';
import { ModelListSkeleton } from './ModelListSkeleton';
import { ModelRow } from './ModelRow';

interface LibraryRowsProps {
  query: string;
  sort: LibrarySort;
  /** What the address has in the inspector, for the picked row. */
  subject: InspectorSubject | null;
  servers: ServerViewModel[];
  paired: PairedModelsState;
  onPickLocal: (id: number) => void;
  onPickFar: (id: number) => void;
  onUnload: (id: number) => void;
}

function Group({ label, aside, children }: { label: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <section aria-label={label} className="flex flex-col pb-md">
      <h3 className="m-0 flex items-center justify-between px-md py-xs text-xs font-semibold text-text-secondary">
        <span>{label}</span>
        {aside}
      </h3>
      {children}
    </section>
  );
}

const Note: FC<{ children: ReactNode }> = ({ children }) => (
  <p className="m-0 px-md py-xs text-xs text-text-secondary">{children}</p>
);

/** The facts under a model's name: params, quant, size, and what it can do. */
function factsOf(model: GgufModel): (string | null | false)[] {
  return [
    formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount),
    model.quantization,
    model.sizeBytes > 0 && formatSize(model.sizeBytes),
    canSee(model) && 'sees images',
    model.imageFamily !== undefined && `draws (${imageFamilyLabel(model.imageFamily)})`,
  ];
}

export const LibraryRows: FC<LibraryRowsProps> = ({ query, sort, subject, servers, paired, onPickLocal, onPickFar, onUnload }) => {
  const { models, loading, error } = useModelLibrary();
  const { snapshot, cancellingId } = useDownloads();
  const { status: remote } = useRemoteState();
  const { showToast } = useToastContext();

  if (loading && models.length === 0) return <ModelListSkeleton />;

  const serverOf = new Map(servers.map((s) => [s.modelId, s]));
  const { loaded, here } = libraryGroups(models, new Set(serverOf.keys()), query, sort);
  const downloads = snapshot ? [...(snapshot.active ? [snapshot.active] : []), ...snapshot.waiting] : [];
  const shownDownloads = downloads.filter((row) => matchesDownload(row, query));
  const far = paired.group?.machine.kind === 'paired' ? paired.group : null;
  const farShown = far ? far.models.filter((m) => matchesFar(m, query)) : [];

  const copyId = (model: GgufModel) =>
    model.openaiId !== null && (
      <CopyValue
        value={model.openaiId}
        label={`Copy the Model ID ${model.openaiId}`}
        onCopied={(id) => showToast(`Copied ${id}`, 'success')}
      />
    );

  const row = (model: GgufModel) => {
    const server = serverOf.get(model.id);
    const state = server?.launch?.state ?? 'loaded';
    const missing = model.missingComponents.length;
    const accessory = server ? (
      <StatusMark kind={RESIDENCY_MARK[state]} word={RESIDENCY_WORDS[state]} />
    ) : missing > 0 ? (
      <StatusMark kind="attention" word={`${missing} ${missing === 1 ? 'part' : 'parts'} missing`} />
    ) : null;
    const unload = (event: MouseEvent) => {
      event.stopPropagation();
      onUnload(model.id);
    };
    return (
      <ModelRow
        key={model.id}
        name={model.name}
        facts={factsOf(model)}
        accessory={accessory}
        actions={
          <>
            {copyId(model)}
            {server && (
              <Button size="sm" variant="ghost" onClick={unload}>
                Unload
              </Button>
            )}
          </>
        }
        selected={subject?.kind === 'model' && subject.id === model.id}
        onSelect={() => onPickLocal(model.id)}
      />
    );
  };

  return (
    <div className="flex flex-col">
      {shownDownloads.length > 0 && (
        <Group label="Downloading">
          {shownDownloads.map((d) => (
            <DownloadRow
              key={d.id}
              row={d}
              cancelling={cancellingId === d.id}
              onCancel={(id) => void cancelDownload(id)}
              onRemove={(id) => void removeFromQueue(id)}
            />
          ))}
        </Group>
      )}

      {error !== null && models.length === 0 ? (
        <div className="flex items-center gap-sm px-md py-sm text-sm text-text-secondary">
          <span>{`Can't read the library: ${error.replace(/\.$/, '')}.`}</span>
          <Button size="sm" variant="ghost" onClick={() => void refreshLibrary()}>
            Retry
          </Button>
        </div>
      ) : models.length === 0 ? (
        <Note>{far ? `No models on this Mac. ${paired.name}'s models below work as they are.` : 'No models on this Mac.'}</Note>
      ) : loaded.length + here.length === 0 ? (
        <Note>Nothing in your library matches.</Note>
      ) : null}

      {loaded.length > 0 && (
        <Group label="Loaded">
          <div role="listbox" aria-label="Loaded">{loaded.map(row)}</div>
        </Group>
      )}
      {here.length > 0 && (
        <Group label="On this Mac">
          <div role="listbox" aria-label="On this Mac">{here.map(row)}</div>
        </Group>
      )}

      {far && (
        <Group
          label={`On ${paired.name}`}
          aside={remote && <StatusMark kind={REACH_MARK[remote.reach]} word={REACH_WORDS[remote.reach]} />}
        >
          {farShown.length === 0 ? (
            <Note>{far.models.length === 0 ? `${paired.name} has no models.` : `None of ${paired.name}'s models match.`}</Note>
          ) : (
            <div role="listbox" aria-label={`On ${paired.name}`}>
              {farShown.map((m) => (
                <ModelRow
                  key={`${far.machine.kind === 'paired' ? far.machine.fingerprint : ''}:${m.gglib_id}`}
                  name={m.id}
                  facts={[m.quantization, canSee(m) && 'sees images', m.description]}
                  selected={subject?.kind === 'far' && subject.id === m.gglib_id}
                  onSelect={() => onPickFar(m.gglib_id)}
                />
              ))}
            </div>
          )}
        </Group>
      )}
    </div>
  );
};
