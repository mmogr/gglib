import { FC, useCallback, useEffect, useState } from 'react';
import { MessageSquare, Rocket, Server } from 'lucide-react';
import type { PairedModelsState } from '../../hooks/usePairedModels';
import { getTransport } from '../../services/transport';
import type { ModelLookup } from '../../types/generated/ModelLookup';
import type { ModelRef } from '../../types/generated/ModelRef';
import { canSee } from '../../utils/canSee';
import { formatError } from '../../utils/errors';
import { Banner } from '../ui/Banner';
import { Button } from '../ui/Button';
import { Chip } from '../ui/Chip';
import { Icon } from '../ui/Icon';
import { VisionChip } from '../VisionChip';
import { ModelMetadataGrid } from './components/ModelMetadataGrid';

interface FarModelInspectorProps {
  /** The paired machine's model picked in the library. */
  model: ModelRef;
  /** That machine's rows: its name, whether it is reached, what may be done there, and a re-read. */
  paired: PairedModelsState;
  /** Open a chat with the model, which runs on that machine, by its name there. */
  onChat: (model: ModelRef, name: string) => void;
}

/** The model's detail as that machine has it, read again on `reload`. */
function useFarDetail(id: number) {
  const [lookup, setLookup] = useState<ModelLookup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let current = true;
    setError(null);
    getTransport()
      .getPairedModel(id)
      .then((read) => current && setLookup(read))
      .catch((err: unknown) => current && setError(formatError(err)));
    return () => {
      current = false;
    };
  }, [id, tick]);
  // A new model is not shown as the last one's while its own is read.
  const shown = lookup?.detail.id === id ? lookup : null;
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { lookup: shown, error, reload };
}

/**
 * The paired machine's model, read-only: what that machine stores about it
 * (with no path on its disk), whether it reads images (the Vision chip its
 * row carries, from the same listing), whether it is serving, and only the
 * actions that machine allows on its models — Chat and Load, each shown only
 * when the table the daemon sent lists it. While its rows are away or stale
 * the actions are offered but disabled, since a press would not reach it. Load
 * reads the model again when it lands, which is how "Serving on" appears.
 *
 * The command that does the same from a terminal is shown as it is typed,
 * since an id copied from here means this machine's model without `--remote`.
 */
export const FarModelInspector: FC<FarModelInspectorProps> = ({ model, paired, onChat }) => {
  const { lookup, error, reload } = useFarDetail(model.id);
  // A Load belongs to the model it was pressed for: another model picked
  // while it is in flight is shown neither its wait nor its failure.
  const [load, setLoad] = useState<{ id: number; busy: boolean; error: string | null } | null>(null);
  const loading = load?.id === model.id && load.busy;
  const loadError = load?.id === model.id ? load.error : null;
  const actions = paired.group?.actions ?? [];
  const reached = paired.reach === 'reached';
  const detail = lookup?.detail ?? null;
  // As its row in the library says it: from what that machine lists, under any of the model's names.
  const sees = paired.group?.models.some((m) => m.gglib_id === model.id && canSee(m)) ?? false;

  const handleLoad = async () => {
    const id = model.id;
    setLoad({ id, busy: true, error: null });
    let failed: string | null = null;
    try {
      await getTransport().loadPairedModel(id);
      reload();
      paired.refetch();
    } catch (err) {
      failed = formatError(err);
    }
    setLoad((now) => (now?.id === id ? { id, busy: false, error: failed } : now));
  };

  return (
    <div className="flex flex-col overflow-hidden relative flex-1 bg-surface md:h-full md:min-h-0">
      <div className="p-md border-b border-border-light shrink-0 flex items-center gap-sm">
        <h2 className="m-0 text-lg font-semibold truncate">{detail?.name ?? `Model ${model.id}`}</h2>
        <Chip size="sm" leftIcon={<Icon icon={Server} size={11} />}>{paired.name}</Chip>
        {sees && <VisionChip />}
      </div>
      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden p-base flex flex-col gap-md">
        {error && <Banner variant="danger">{`Could not read it from ${paired.name}: ${error}`}</Banner>}
        {!reached && (
          <Banner variant="warning">
            {paired.reach === 'away'
              ? `${paired.name} is away; this is what it said when last reached.`
              : `${paired.name}'s models could not be read just now; this may be out of date.`}
          </Banner>
        )}
        {detail?.isServing && (
          <p className="m-0 text-sm text-text-secondary">{`Serving on ${paired.name}.`}</p>
        )}
        {detail && <ModelMetadataGrid model={detail} detail={detail} />}
        <p className="m-0 text-xs text-text-muted">
          From a terminal here: <code className="font-mono">{`gglib chat ${model.id} --remote`}</code>
        </p>
        {loadError && <Banner variant="danger">{`Could not load it: ${loadError}`}</Banner>}
      </div>
      <div className="flex items-center gap-sm flex-wrap p-base border-t border-border bg-background shrink-0">
        {actions.includes('chat') && (
          <Button
            variant="primary"
            size="lg"
            disabled={!reached || !detail}
            onClick={() => detail && onChat(model, detail.name)}
            leftIcon={<Icon icon={MessageSquare} size={16} />}
          >
            Chat
          </Button>
        )}
        {actions.includes('load') && !detail?.isServing && (
          <Button
            variant="secondary"
            size="lg"
            disabled={!reached || loading}
            onClick={() => void handleLoad()}
            leftIcon={<Icon icon={Rocket} size={16} />}
          >
            {loading ? 'Loading…' : `Load on ${paired.name}`}
          </Button>
        )}
      </div>
    </div>
  );
};
