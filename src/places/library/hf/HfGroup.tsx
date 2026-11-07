/**
 * The Hugging Face group, under the library's own: what the one search's
 * Enter found; a search's results come with `[Chat · Image]` and their own
 * `Sort ▾`.
 *
 * With the field empty it holds the model `gglib up` would suggest for this
 * machine, from the daemon's shortlist, with the figures it was sized by.
 * After Enter it holds the Hub's repos as rows (a pick opens the repo's
 * preview in the inspector) and "More results" while the Hub has more, or
 * a `user/repo:QUANT` download as its one row, or why it failed.
 *
 * Every figure is the daemon's: a repo's parameter count is the summary's
 * (#1740: the Hub's, which can be a draft's), whether it calls tools is the
 * daemon's reading of its chat template, the suggestion's budget is named
 * in the words `gglib up` prints it in, and a refusal is said as it came.
 *
 * @module places/library/hf/HfGroup
 */

import { useEffect, useState, type FC, type ReactNode } from 'react';
import { Button } from '../../../components/ui/Button';
import { Chip } from '../../../components/ui/Chip';
import { useToastContext } from '../../../contexts/ToastContext';
import { useToolSupportCache } from '../../../hooks/useToolSupportCache';
import { queueDownload } from '../../../services/downloadsEvents';
import { useModelLibrary } from '../../../services/modelRegistry';
import { getRecommendedModel } from '../../../services/transport/api/setup';
import type { HfModelKind, HfModelSummary } from '../../../types';
import type { RecommendationDto } from '../../../types/setup';
import { formatError } from '../../../utils/errors';
import { formatNumber, formatParamCount, formatRecommendation } from '../../../utils/format';
import { Group, Note } from '../Group';
import { Menu } from '../Menu';
import { ModelListSkeleton } from '../ModelListSkeleton';
import { ModelRow } from '../ModelRow';
import { HF_SORTS } from './useHuggingFaceSearch';
import type { LibrarySearch } from './useLibrarySearch';

const KINDS: { id: HfModelKind; label: string }[] = [
  { id: 'chat', label: 'Chat' },
  { id: 'image', label: 'Image' },
];

interface HfGroupProps {
  search: LibrarySearch;
  /** The repo in the inspector. */
  pickedId: string | null;
  onPick: (model: HfModelSummary) => void;
  /** The daemon would refuse another download. */
  queueFull: boolean;
}

/** Queue a download, and say it was queued or why not. */
function useQueue() {
  const { showToast } = useToastContext();
  return async (repo: string, quant?: string): Promise<boolean> => {
    try {
      await queueDownload(repo, quant);
      showToast(`Download queued: ${repo}`, 'success');
      return true;
    } catch (err) {
      showToast(formatError(err), 'error');
      return false;
    }
  };
}

/** A row with a name, a line under it, and one control always shown. */
const ActionRow: FC<{ name: string; line?: string; action: ReactNode }> = ({ name, line, action }) => (
  <div className="flex min-h-[52px] items-center gap-md px-md py-sm">
    <div className="flex min-w-0 flex-1 flex-col">
      <span className="truncate font-mono text-sm text-text">{name}</span>
      {line && <span className="text-xs text-text-secondary">{line}</span>}
    </div>
    {action}
  </div>
);

/** The suggestion for this machine, or that nothing on the shortlist fits. */
const Suggestion: FC<{ queueFull: boolean }> = ({ queueFull }) => {
  // `undefined` while asked; a failed read shows nothing, since it says
  // nothing about what fits.
  const [answer, setAnswer] = useState<RecommendationDto | null | 'failed' | undefined>(undefined);
  const { models } = useModelLibrary();
  const queue = useQueue();
  useEffect(() => {
    let live = true;
    getRecommendedModel()
      .then((rec) => live && setAnswer(rec))
      .catch(() => live && setAnswer('failed'));
    return () => {
      live = false;
    };
  }, []);

  if (answer === undefined || answer === 'failed') return null;
  if (answer === null) return <Note>Nothing gglib would suggest fits. Search for a smaller model below.</Note>;
  return (
    <section aria-label="Suggested for this Mac">
      <Note>Suggested for this Mac</Note>
      <ActionRow
        name={answer.repo}
        line={formatRecommendation(answer)}
        action={
          <Button
            size="sm"
            variant={models.length === 0 ? 'primary' : 'secondary'}
            disabled={queueFull}
            onClick={() => void queue(answer.repo)}
          >
            Download
          </Button>
        }
      />
    </section>
  );
};

/** One repo: its id, then params, tool calling and downloads. */
const HfRow: FC<{ model: HfModelSummary; picked: boolean; onPick: () => void }> = ({ model, picked, onPick }) => {
  const { supports } = useToolSupportCache(model.id);
  return (
    <ModelRow
      name={model.id}
      facts={[
        model.parameters_b !== null && formatParamCount(model.parameters_b),
        supports === true && 'calls tools',
        `${formatNumber(model.downloads)} downloads`,
      ]}
      selected={picked}
      onSelect={onPick}
    />
  );
};

export const HfGroup: FC<HfGroupProps> = ({ search, pickedId, onPick, queueFull }) => {
  const { shown, hf, openError, setQuery } = search;
  const queue = useQueue();
  // Nothing to say while a named repo is still being read.
  if (shown === null || (shown.kind === 'open' && openError === null)) return null;

  // Kind and sort act on a search, so they are drawn only with one.
  const controls = shown.kind === 'search' && (
    <span className="flex items-center gap-xs">
      <span role="group" aria-label="Kind" className="flex items-center gap-xs">
        {KINDS.map((k) => (
          <Chip key={k.id} selected={hf.kind === k.id} onClick={() => hf.setKind(k.id)}>
            {k.label}
          </Chip>
        ))}
      </span>
      <Menu
        label="Sort"
        name="Sort Hugging Face"
        items={[
          ...HF_SORTS.map((s) => ({
            label: s.label,
            checked: hf.sort.by === s.value,
            onPick: () => hf.setSort({ by: s.value, ascending: s.ascending }),
          })),
          { label: 'Ascending', checked: hf.sort.ascending, gapBefore: true, onPick: () => hf.setSort({ ...hf.sort, ascending: true }) },
          { label: 'Descending', checked: !hf.sort.ascending, onPick: () => hf.setSort({ ...hf.sort, ascending: false }) },
        ]}
      />
    </span>
  );

  let body: ReactNode;
  switch (shown.kind) {
    case 'suggestion':
      body = <Suggestion queueFull={queueFull} />;
      break;
    case 'open':
      body = <Note>{openError}</Note>;
      break;
    case 'download':
      body = (
        <ActionRow
          name={`Download ${shown.quant} from ${shown.repo}`}
          action={
            <Button
              size="sm"
              variant="secondary"
              disabled={queueFull}
              onClick={() => void queue(shown.repo, shown.quant).then((ok) => ok && setQuery(''))}
            >
              Download
            </Button>
          }
        />
      );
      break;
    case 'search':
      body = hf.loading ? (
        <ModelListSkeleton />
      ) : hf.error !== null ? (
        <Note>{hf.error}</Note>
      ) : (
        <>
          {hf.models.length === 0 && !hf.hasMore && <Note>{`No GGUF repos match '${shown.query}'.`}</Note>}
          {hf.models.length > 0 && (
            <div role="listbox" aria-label="Hugging Face">
              {hf.models.map((m) => (
                <HfRow key={m.id} model={m} picked={m.id === pickedId} onPick={() => onPick(m)} />
              ))}
            </div>
          )}
          {hf.hasMore && (
            <Button variant="ghost" size="sm" className="mx-md self-start" isLoading={hf.loadingMore} onClick={hf.loadMore}>
              More results
            </Button>
          )}
        </>
      );
  }

  return (
    <Group label="Hugging Face" aside={controls}>
      {body}
    </Group>
  );
};
