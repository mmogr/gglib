import { ChangeEvent, FC, useEffect, useState } from 'react';
import { Select } from '../../ui/Select';
import { useServers } from '../../../hooks/useServers';
import { getTransport } from '../../../services/transport';
import { useToastContext } from '../../../contexts/ToastContext';
import { formatError } from '../../../utils/errors';
import type { GgufModel } from '../../../types';

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
}

/**
 * The composer margin's model: a picker while the chat is on this machine.
 *
 * It lists the servers running here, then the registered models that are
 * not. Choosing one hands it to the page, which moves the chat there,
 * starting the model first if it is not running. A chat with another machine
 * has nothing here to pick from, so it names its model as text.
 */
export const ModelPicker: FC<ModelPickerProps> = ({ modelId, modelName, quantization, onPick, starting = null }) => {
  const { servers } = useServers();
  const { showToast } = useToastContext();
  const [models, setModels] = useState<GgufModel[]>([]);
  const canPick = onPick !== undefined && modelId !== undefined;

  useEffect(() => {
    if (!canPick) return;
    let cancelled = false;
    getTransport().listModels()
      .then((list) => { if (!cancelled) setModels(list); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [canPick]);

  const quant = quantization && <span className="font-mono">{quantization}</span>;
  if (!canPick) {
    return (
      <>
        <span className="font-mono text-sm font-medium text-text-secondary">{modelName}</span>
        {quant}
      </>
    );
  }

  const running = servers.filter((s) => s.modelId !== modelId);
  const runningIds = new Set(servers.map((s) => s.modelId));
  const idle = models.filter((m) => m.id !== modelId && !runningIds.has(m.id));

  const handleChange = async (event: ChangeEvent<HTMLSelectElement>) => {
    const id = Number(event.target.value);
    const server = running.find((s) => s.modelId === id);
    const name = server?.modelName ?? idle.find((m) => m.id === id)?.name;
    if (name === undefined) return;
    try {
      await onPick({ modelId: id, modelName: name });
    } catch (error) {
      // The chat stays on the model it was on.
      showToast(`Could not start ${name}: ${formatError(error)}`, 'error');
    }
  };

  return (
    <>
      <Select
        aria-label="Model"
        size="sm"
        value={modelId}
        onChange={(event) => void handleChange(event)}
        disabled={starting !== null}
        className="font-mono font-medium text-text-secondary bg-transparent border-transparent max-w-[220px] truncate"
      >
        <option value={modelId}>{modelName}</option>
        {running.length > 0 && (
          <optgroup label="Running">
            {running.map((s) => <option key={s.modelId} value={s.modelId}>{s.modelName}</option>)}
          </optgroup>
        )}
        {idle.length > 0 && (
          <optgroup label="Not running">
            {idle.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
          </optgroup>
        )}
      </Select>
      {starting ? <span role="status">Starting {starting}…</span> : quant}
    </>
  );
};
