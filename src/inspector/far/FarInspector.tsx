/**
 * The paired machine's model, read-only: its name (⧉ copies it), the
 * machine and its reach (`REACH_WORDS` for the remote status's `reach`, as
 * the library's group heads it), the summary its row carries, and only the
 * actions that machine lists for its models: Load on <machine> and Chat.
 * While its rows are away or could not be read the actions stay, disabled,
 * since a press would not reach it; away says why, and a failed read is one
 * line with Retry. Load reads the model again when it lands, which is how
 * "● Loaded on <machine>" appears; a Load's wait and failure belong to the
 * model it was pressed for. Details holds what that machine stores (no path
 * on its disk) and the command that does the same from a terminal here.
 *
 * @module inspector/far/FarInspector
 */

import { useCallback, useEffect, useState, type FC } from 'react';
import { useNavigate } from 'react-router';
import { X } from 'lucide-react';
import { chatPathOnFar } from '../../app/openChat';
import { paths } from '../../app/paths';
import { CopyValue } from '../../components/CopyValue';
import { REACH_MARK, StatusMark } from '../../components/status/StatusMark';
import { REACH_WORDS, RESIDENCY_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useToastContext } from '../../contexts/ToastContext';
import type { PairedModelsState } from '../../hooks/usePairedModels';
import { useRemoteState } from '../../services/remoteRegistry';
import { getTransport } from '../../services/transport';
import type { ModelLookup } from '../../types/generated/ModelLookup';
import { canSee } from '../../utils/canSee';
import { formatError } from '../../utils/errors';
import { FarDetails } from './FarDetails';

/** The model's detail as that machine has it, read again on `reload`. */
function useFarDetail(id: number) {
  const [lookup, setLookup] = useState<ModelLookup | null>(null);
  const [failed, setFailed] = useState(false);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let current = true;
    setFailed(false);
    getTransport()
      .getPairedModel(id)
      .then((read) => current && setLookup(read))
      .catch(() => current && setFailed(true));
    return () => {
      current = false;
    };
  }, [id, tick]);
  // A new model is not shown as the last one's while its own is read.
  const shown = lookup?.detail.id === id ? lookup : null;
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { detail: shown?.detail ?? null, failed, reload };
}

interface FarInspectorProps {
  /** The model's id on the paired machine. */
  id: number;
  /** That machine's rows: its name, whether they are current, what may be done there, and a re-read. */
  paired: PairedModelsState;
}

export const FarInspector: FC<FarInspectorProps> = ({ id, paired }) => {
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const { status } = useRemoteState();
  const { detail, failed, reload } = useFarDetail(id);
  const [load, setLoad] = useState<{ id: number; busy: boolean; error: string | null } | null>(null);
  const loading = load?.id === id && load.busy;
  const loadError = load?.id === id ? load.error : null;
  const actions = paired.group?.actions ?? [];
  const reached = paired.reach === 'reached';
  const row = paired.group?.models.find((m) => m.gglib_id === id);
  const name = detail?.name ?? row?.id ?? `Model ${id}`;
  const reach = status?.reach ?? 'disconnected';
  // As its row in the library says it: from what that machine lists.
  const summary = row ? [row.quantization, canSee(row) && 'sees images', row.description].filter(Boolean).join(' · ') : '';

  const handleLoad = async () => {
    setLoad({ id, busy: true, error: null });
    let error: string | null = null;
    try {
      await getTransport().loadPairedModel(id);
      reload();
      paired.refetch();
    } catch (err) {
      error = formatError(err);
    }
    setLoad((now) => (now?.id === id ? { id, busy: false, error } : now));
  };

  const retry = () => {
    reload();
    paired.refetch();
  };

  const loadButton = actions.includes('load') && !detail?.isServing && (
    <Button disabled={!reached || loading} isLoading={loading} onClick={() => void handleLoad()}>
      {`Load on ${paired.name}`}
    </Button>
  );
  const chatButton = actions.includes('chat') && (
    <Button variant={detail?.isServing ? 'primary' : 'secondary'} disabled={!reached || !detail} onClick={() => paired.group && void chatPathOnFar({ machine: paired.group.machine, id }).then((to) => navigate(to))}>
      Chat
    </Button>
  );

  return (
    <div className="flex flex-col gap-lg p-md">
      <div className="flex flex-col gap-sm">
        <div className="flex flex-col gap-xs">
          <div className="flex items-center gap-xs">
            <h2 className="m-0 min-w-0 truncate text-base font-semibold text-text">{name}</h2>
            <CopyValue value={name} label={`Copy the name ${name}`} onCopied={() => showToast(`Copied ${name}`, 'success')} />
            <span className="flex-1" />
            <Button variant="ghost" size="sm" iconOnly aria-label="Close" title="Close" onClick={() => navigate(paths.library())}>
              <Icon icon={X} size={14} />
            </Button>
          </div>
          <p className="m-0 flex items-center gap-xs text-xs text-text-secondary">
            {`on ${paired.name} ·`}
            <StatusMark kind={REACH_MARK[reach]} word={REACH_WORDS[reach]} />
          </p>
          {summary && <p className="m-0 text-xs text-text-secondary">{summary}</p>}
        </div>
        {(loadButton || chatButton) && (
          <div className="flex items-center gap-sm">
            {loadButton}
            {chatButton}
          </div>
        )}
        {paired.reach === 'away' && <p className="m-0 text-xs text-text-secondary">{`${paired.name} is away; Load and Chat wait until it is back.`}</p>}
        {detail?.isServing && <StatusMark kind="on" word={`${RESIDENCY_WORDS.loaded} on ${paired.name}`} className="text-xs" />}
        {(failed || paired.reach === 'stale') && (
          <p role="alert" className="m-0 flex items-center gap-sm text-xs text-text-secondary">
            {`${failed ? 'It' : `${paired.name}'s models`} could not be read just now.`}
            <Button variant="link" size="sm" className="text-xs font-normal" onClick={retry}>
              Retry
            </Button>
          </p>
        )}
        {loadError && (
          <p role="alert" className="m-0 flex items-baseline gap-xs text-xs text-text">
            <StatusMark kind="failed" label="Failed" />
            {`Didn't load: ${loadError}`}
          </p>
        )}
      </div>
      {detail && <FarDetails id={id} detail={detail} />}
    </div>
  );
};
