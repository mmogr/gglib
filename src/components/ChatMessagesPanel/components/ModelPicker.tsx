import { FC } from 'react';
import { Menu, type MenuItem } from '../../ui/Menu';
import { RESIDENCY_WORDS } from '../../status/words';
import { useServers } from '../../../hooks/useServers';
import { usePairedModels } from '../../../hooks/usePairedModels';
import { useModelLibrary } from '../../../services/modelRegistry';
import { useToastContext } from '../../../contexts/ToastContext';
import { formatError } from '../../../utils/errors';

/** A model to move the chat to, as the picker names it. */
export interface ModelChoice {
  modelId: number;
  modelName: string;
}

interface ModelPickerProps {
  /** The model the chat is on; absent for a chat with another machine. */
  modelId?: number;
  modelName: string;
  quantization?: string | null;
  /** Move the chat to another model; a failure is toasted here. Absent where there is nothing to pick. */
  onPick?: (choice: ModelChoice) => Promise<void>;
  /**
   * The model a switch is starting. Held by the page's session, not here:
   * this remounts with each conversation, and must stay locked across that.
   */
  starting?: string | null;
  /** Unload the model the chat is on; offered only while it is loaded. */
  onUnload?: () => Promise<void>;
  /** Open the model's inspector, where its log is. */
  onShow?: () => void;
  /** Open a new chat on the paired machine's model, by its id there; offered only in a new chat. */
  onOpenFar?: (id: number) => void;
}

/** Why a saved chat cannot move to the paired machine's model. */
const STAYS = 'A chat stays on its machine. Start a new chat to use this model.';

/**
 * The composer's model: a picker ▾ while the chat is on this machine.
 *
 * Its groups are Loaded (the servers running here, the chat's own ticked),
 * On this Mac (the registered models that are not), and the paired
 * machine's models when one is connected; then Unload <model>, which stops
 * it for every client of the endpoint and leaves the chat open, and Show
 * <model>, which opens its inspector, where its log is. Choosing a model
 * here hands it to the page, which moves the chat there, starting the model
 * first if it is not running; meanwhile the picker is locked and says so.
 * A chat's machine is fixed, so a paired machine's model opens a new chat
 * on it from a new chat, and is listed greyed, with why, in a saved one. A chat with another machine has
 * nothing here to pick, so it names its model as text.
 */
export const ModelPicker: FC<ModelPickerProps> = ({
  modelId,
  modelName,
  quantization,
  onPick,
  starting = null,
  onUnload,
  onShow,
  onOpenFar,
}) => {
  const { servers } = useServers();
  const { showToast } = useToastContext();
  const { models } = useModelLibrary();
  const canPick = onPick !== undefined && modelId !== undefined;
  const paired = usePairedModels(canPick);

  if (!canPick) {
    return <span className="truncate px-sm text-sm text-text-secondary">{modelName}</span>;
  }

  if (starting !== null) {
    return (
      <span role="status" className="truncate px-sm text-sm text-text-secondary">
        {RESIDENCY_WORDS.loading} {starting}…
      </span>
    );
  }

  const pick = async (choice: ModelChoice) => {
    try {
      await onPick(choice);
    } catch (error) {
      // The chat stays on the model it was on.
      showToast(`Could not start ${choice.modelName}: ${formatError(error)}`, 'error');
    }
  };

  const unload = async () => {
    try {
      await onUnload?.();
    } catch (error) {
      showToast(`Could not unload ${modelName}: ${formatError(error)}`, 'error');
    }
  };

  const runningIds = new Set(servers.map((s) => s.modelId));
  const loaded: MenuItem[] = [
    ...(runningIds.has(modelId) ? [] : [{ label: modelName, checked: true, onPick: () => {} }]),
    ...servers.map((s) => ({
      label: s.modelName,
      checked: s.modelId === modelId,
      onPick: () => (s.modelId === modelId ? undefined : void pick({ modelId: s.modelId, modelName: s.modelName })),
    })),
  ];
  const idle = models.filter((m) => m.id !== modelId && !runningIds.has(m.id));
  const far = paired.group?.models ?? [];

  const items: MenuItem[] = [
    { label: RESIDENCY_WORDS.loaded, heading: true },
    ...loaded,
    ...(idle.length > 0 ? [{ label: 'On this Mac', heading: true }] : []),
    ...idle.map((m) => ({ label: m.name, checked: false, onPick: () => void pick({ modelId: m.id, modelName: m.name }) })),
    ...(far.length > 0 ? [{ label: `On ${paired.name}`, heading: true }] : []),
    ...far.map((m) => ({
      label: m.id,
      disabled: onOpenFar === undefined,
      title: onOpenFar === undefined ? STAYS : `Opens a new chat on ${m.id}, on ${paired.name}.`,
      onPick: () => onOpenFar?.(m.gglib_id),
    })),
    ...(onUnload
      ? [{
          label: `Unload ${modelName}`,
          gapBefore: true,
          title: 'Unload this model. Anything using it through the endpoint, Copilot included, loses it. The chat stays open.',
          onPick: () => void unload(),
        }]
      : []),
    ...(onShow ? [{ label: `Show ${modelName}`, gapBefore: !onUnload, onPick: onShow }] : []),
  ];

  return (
    <Menu
      label={<span className="truncate">{modelName}</span>}
      name={`Model: ${modelName}`}
      title={quantization ? `${modelName} · ${quantization}` : modelName}
      align="end"
      side="above"
      className="max-w-[240px] text-sm font-normal"
      items={items}
    />
  );
};
