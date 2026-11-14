import { useEffect, useState } from 'react';
import { CAPABILITY_FLAGS } from '../readings/capabilityFlags';
import { getTransport } from '../services/transport';
import { canSee } from '../utils/canSee';
import { thinks as modelThinks } from '../utils/thinks';

export interface ChatModelFacts {
  /**
   * Its stored tool-call flag, from its catalogue entry. null = unknown
   * (permissive fallback - never gates tools when status is uncertain).
   */
  supportsToolCalls: boolean | null;
  /** The model's quantisation, from its catalogue entry; the composer says it. */
  quantization: string | null;
  /** Whether it reads images, from its catalogue entry; null = not known yet. */
  sees: boolean | null;
  /** The context it is served with, from its catalogue entry, when known. */
  contextLength: number | null;
  /** Whether it thinks, by its catalogue entry's tags; false until that is known. */
  thinks: boolean;
}

/**
 * What the chat page knows about the model it is talking to, from its
 * catalogue entry: whether it calls tools, its quantisation, whether it reads
 * images, its context (its served default, else the file's), and whether it
 * thinks.
 *
 * Fetched once per model id. A page that moves to another model is remounted
 * with the new id (the chat route keys it by the session), so the
 * answers never describe a model the page is no longer talking to.
 */
export function useChatModelFacts(modelId: number | undefined): ChatModelFacts {
  const [supportsToolCalls, setSupportsToolCalls] = useState<boolean | null>(null);
  const [quantization, setQuantization] = useState<string | null>(null);
  const [sees, setSees] = useState<boolean | null>(null);
  const [contextLength, setContextLength] = useState<number | null>(null);
  const [thinks, setThinks] = useState(false);
  useEffect(() => {
    // Nothing to ask about remotely: the capability is read from this
    // machine's catalogue and the model is on the other machine.
    // `null` is already the permissive answer, which is the right one here.
    if (modelId === undefined) return;
    let cancelled = false;
    // A failed read leaves every answer unknown, supportsToolCalls null.
    getTransport().getModel(modelId)
      .then((model) => {
        if (cancelled || !model) return;
        setSupportsToolCalls((model.capabilities & CAPABILITY_FLAGS.supportsToolCalls) !== 0);
        setQuantization(model.quantization ?? null);
        setSees(canSee(model));
        setContextLength(model.serverDefaults?.contextLength ?? model.contextLength ?? null);
        setThinks(modelThinks(model));
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [modelId]);
  return { supportsToolCalls, quantization, sees, contextLength, thinks };
}
