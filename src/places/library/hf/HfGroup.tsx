/**
 * The Hugging Face group, under the library's own: what the one search's
 * Enter found; a search's results come with `[Chat · Image]` and their own
 * `Sort ▾`.
 *
 * With the field empty it holds the model `gglib up` would suggest for this
 * machine, from the daemon's shortlist, with the figures it was sized by;
 * it can be dismissed, and stays dismissed until the shortlist suggests
 * another repo.
 * After Enter it holds the Hub's repos as rows (a pick opens the repo's
 * preview in the inspector; with the arrow keys, the repo a run of keys
 * ends on, `KEY_PICK_MS` after the last) and "More results" while the Hub has more, or
 * a `user/repo:QUANT` download as its one row, or why it failed. A search
 * or a repo Hugging Face rate limits says when the limit lifts in the same
 * place, and its answer replaces the line when it comes; a "More results"
 * that waits or fails says so under the rows already listed.
 *
 * Every figure is the daemon's: a repo's parameter count is the summary's
 * (#1740: the Hub's, which can be a draft's), whether it calls tools is the
 * summary's reading of its chat template, the suggestion's budget is named
 * in the words `gglib up` prints it in, and a refusal is said as it came.
 *
 * @module places/library/hf/HfGroup
 */

import { useEffect, useRef, useState, type FC, type ReactNode } from 'react';
import { X } from 'lucide-react';
import { Button } from '../../../components/ui/Button';
import { Icon } from '../../../components/ui/Icon';
import { HubWaitLine } from '../../../components/status/HubWaitLine';
import { IconButton } from '../../../components/ui/IconButton';
import { Chip } from '../../../components/ui/Chip';
import { Listbox, tabStop } from '../../../components/ui/Listbox';
import { useToastContext } from '../../../contexts/ToastContext';
import { queueDownload } from '../../../services/downloadsEvents';
import { useModelLibrary } from '../../../services/modelRegistry';
import { useThisMachine } from '../../../services/hostRegistry';
import { getRecommendedModel, getSetupStatus } from '../../../services/transport/api/setup';
import type { HfModelKind, HfModelSummary } from '../../../types';
import type { RecommendationDto } from '../../../types/setup';
import { formatError } from '../../../utils/errors';
import { formatNumber, formatParamCount, formatRecommendation } from '../../../utils/format';
import { Group, Note } from '../Group';
import { Menu } from '../../../components/ui/Menu';
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
  /** `replace`: picked with the arrow keys from the repo picked before. */
  onPick: (model: HfModelSummary, replace?: boolean) => void;
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

const DISMISSED_KEY = 'gglib.library.suggestionDismissed';

/** The suggested repo this viewer dismissed, if any. */
function dismissedRepo(): string | null {
  try {
    return window.localStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

/**
 * The suggestion for this machine, asked each time the field empties.
 * `undefined` while asked, and also for a failed read (it says nothing about
 * what fits) and a dismissed suggestion: the group then draws nothing.
 * A dismissal is remembered by repo, so a new suggestion is shown again.
 */
function useSuggestion(asked: boolean) {
  const [answer, setAnswer] = useState<RecommendationDto | null | undefined>(undefined);
  // Why nothing is suggested when it is not that nothing fits, the daemon's
  // words, read only when nothing is.
  const [noneReason, setNoneReason] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(dismissedRepo);
  useEffect(() => {
    if (!asked) return;
    let live = true;
    getRecommendedModel()
      .then(async (rec) => {
        if (rec === null) {
          const reason = await getSetupStatus()
            .then((status) => status.noSuggestionReason ?? null)
            .catch(() => null);
          if (live) setNoneReason(reason);
        }
        if (live) setAnswer(rec);
      })
      .catch(() => live && setAnswer(undefined));
    return () => {
      live = false;
    };
  }, [asked]);
  const dismiss = (repo: string) => {
    setDismissed(repo);
    try {
      window.localStorage.setItem(DISMISSED_KEY, repo);
    } catch {
      // A viewer without storage dismisses it until the library is left.
    }
  };
  return { answer: answer && answer.repo === dismissed ? undefined : answer, noneReason, dismiss };
}

/** The suggestion for this machine, or that nothing on the shortlist fits. */
const Suggestion: FC<{ answer: RecommendationDto | null; noneReason: string | null; queueFull: boolean; onDismiss: (repo: string) => void }> = ({
  answer,
  noneReason,
  queueFull,
  onDismiss,
}) => {
  const { models } = useModelLibrary();
  const queue = useQueue();
  const machine = useThisMachine();
  if (answer === null) return <Note>{noneReason ?? "Nothing gglib would suggest fits. Search for a smaller model below."}</Note>;
  return (
    <section aria-label={`Suggested for ${machine.phrase}`}>
      <Note>{`Suggested for ${machine.phrase}`}</Note>
      <ActionRow
        name={answer.repo}
        line={formatRecommendation(answer)}
        action={
          <span className="flex shrink-0 items-center gap-xs">
            <Button
              size="sm"
              variant={models.length === 0 ? 'primary' : 'secondary'}
              disabled={queueFull}
              onClick={() => void queue(answer.repo, answer.quantization)}
            >
              Download
            </Button>
            <IconButton label="Dismiss suggestion" size="sm" onClick={() => onDismiss(answer.repo)}>
              <Icon icon={X} size={14} />
            </IconButton>
          </span>
        }
      />
    </section>
  );
};

/** One repo: its id, then params, tool calling and downloads. */
const HfRow: FC<{ model: HfModelSummary; picked: boolean; tabIndex: 0 | -1; onPick: () => void }> = ({ model, picked, tabIndex, onPick }) => (
  <ModelRow
    name={model.id}
    facts={[
      model.parameters_b !== null && formatParamCount(model.parameters_b),
      model.supports_tool_calls && 'calls tools',
      `${formatNumber(model.downloads)} downloads`,
    ]}
    selected={picked}
    tabIndex={tabIndex}
    onSelect={onPick}
  />
);

/**
 * How long a pick made with the arrow keys waits before the address follows
 * it, in ms: a run of keys opens the repo it ends on, and the inspector reads
 * that one from the Hub rather than each repo passed over.
 */
const KEY_PICK_MS = 150;

export const HfGroup: FC<HfGroupProps> = ({ search, pickedId, onPick, queueFull }) => {
  const { shown, hf, openError, openLiftsAt, setQuery } = search;
  const queue = useQueue();
  const suggestion = useSuggestion(shown?.kind === 'suggestion');
  // The key pick waiting, and whether the run it ends began on the picked repo.
  const pending = useRef<{ timer: ReturnType<typeof setTimeout>; replace: boolean } | null>(null);
  /** Forget a key pick still waiting: a click, a new search or an opened repo says where the person is now. */
  const cancelKeyPick = () => {
    clearTimeout(pending.current?.timer);
    pending.current = null;
  };
  useEffect(() => {
    return cancelKeyPick;
  }, []);
  useEffect(cancelKeyPick, [hf.models, shown?.kind]);
  const pickByKey = (model: HfModelSummary, replace: boolean) => {
    const run = pending.current;
    clearTimeout(run?.timer);
    const keep = run ? run.replace : replace;
    const timer = setTimeout(() => {
      pending.current = null;
      onPick(model, keep);
    }, KEY_PICK_MS);
    pending.current = { timer, replace: keep };
  };
  // Nothing to say while a named repo is still being read, or while the
  // suggestion is asked, failed or dismissed.
  if (shown === null || (shown.kind === 'open' && openError === null && openLiftsAt === null)) return null;
  if (shown.kind === 'suggestion' && suggestion.answer === undefined) return null;

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
      body = <Suggestion answer={suggestion.answer ?? null} noneReason={suggestion.noneReason} queueFull={queueFull} onDismiss={suggestion.dismiss} />;
      break;
    case 'open':
      body = openLiftsAt !== null ? <HubWaitLine liftsAt={openLiftsAt} className="px-md py-xs" /> : <Note>{openError}</Note>;
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
      ) : (
        <>
          {hf.models.length === 0 && !hf.hasMore && hf.error === null && hf.liftsAt === null && (
            <Note>{`No GGUF repos match '${shown.query}'.`}</Note>
          )}
          {hf.models.length > 0 && (
            <Listbox label="Hugging Face" onPick={(i, replace) => pickByKey(hf.models[i], replace)}>
              {hf.models.map((m, i) => (
                <HfRow
                  key={m.id}
                  model={m}
                  picked={m.id === pickedId}
                  tabIndex={tabStop(i, hf.models.findIndex((h) => h.id === pickedId))}
                  onPick={() => {
                    cancelKeyPick();
                    onPick(m);
                  }}
                />
              ))}
            </Listbox>
          )}
          {hf.liftsAt !== null ? <HubWaitLine liftsAt={hf.liftsAt} className="px-md py-xs" /> : hf.error !== null && <Note>{hf.error}</Note>}
          {hf.hasMore && hf.liftsAt === null && (
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
