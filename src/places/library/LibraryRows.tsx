/**
 * The library's groups, in order: Downloading, Loaded, On this Mac, then the
 * paired machine's. A model is in one of them and moves when its state does
 * (`groups.ts`). Each group of models is a listbox; its rows are options.
 *
 * Every word on a row is the daemon's or arranged from its facts: the state
 * beside a Loaded row is `RESIDENCY_WORDS` for the state the proxy's live
 * dashboard gives its resident (`useResidency`), `Loaded` while the stream
 * says nothing of it (a server is announced once it answers), the paired
 * machine's reach is
 * `REACH_WORDS` for the remote status's `reach`, and a download is said in
 * its row's own text.
 *
 * @module places/library/LibraryRows
 */

import type { FC, MouseEvent } from 'react';
import { CopyValue } from '../../components/CopyValue';
import { RESIDENCY_MARK, REACH_MARK, StatusMark } from '../../components/status/StatusMark';
import { REACH_WORDS, RESIDENCY_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import type { PairedModelsState } from '../../hooks/usePairedModels';
import type { ServerViewModel } from '../../hooks/useServers';
import { refreshModels } from '../../services/modelEvents';
import { useModelLibrary } from '../../services/modelRegistry';
import { useRemoteState } from '../../services/remoteRegistry';
import type { GgufModel } from '../../types';
import type { InspectorSubject } from '../../app/paths';
import { canSee } from '../../utils/canSee';
import { Downloads } from './DownloadRow';
import { Group, Note } from './Group';
import { libraryGroups, matchesFar, modelFacts, type LibrarySort } from './groups';
import { ModelListSkeleton } from './ModelListSkeleton';
import { ModelRow } from './ModelRow';
import { useResidency } from './useResidency';

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

export const LibraryRows: FC<LibraryRowsProps> = ({ query, sort, subject, servers, paired, onPickLocal, onPickFar, onUnload }) => {
  const { models, loading, error } = useModelLibrary();
  const residency = useResidency();
  const { status: remote } = useRemoteState();

  if (loading && models.length === 0) return <ModelListSkeleton />;

  const serverOf = new Map(servers.map((s) => [s.modelId, s]));
  const { loaded, here } = libraryGroups(models, new Set(serverOf.keys()), query, sort);
  const far = paired.group?.machine.kind === 'paired' ? paired.group : null;
  const farShown = far ? far.models.filter((m) => matchesFar(m, query)) : [];

  const copyId = (model: GgufModel) =>
    model.openaiId !== null && (
      <CopyValue value={model.openaiId} label={`Copy the Model ID ${model.openaiId}`} />
    );

  const row = (model: GgufModel) => {
    const server = serverOf.get(model.id);
    const state = residency.get(model.id) ?? 'loaded';
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
        facts={modelFacts(model)}
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
      <Downloads query={query} />

      {error !== null && models.length === 0 ? (
        <div className="flex items-center gap-sm px-md py-sm text-sm text-text-secondary">
          <span>{`Can't read the library: ${error.replace(/\.$/, '')}.`}</span>
          <Button size="sm" variant="ghost" onClick={() => void refreshModels()}>
            Retry
          </Button>
        </div>
      ) : models.length === 0 ? (
        <Note>{far ? `No models on this Mac. ${paired.name}'s models below work as they are.` : 'No models on this Mac.'}</Note>
      ) : loaded.length + here.length === 0 ? (
        <Note>Nothing in your library matches. Press ↵ to search Hugging Face.</Note>
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
