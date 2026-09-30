import { useEffect, useState } from 'react';
import { getTransport } from '../services/transport';

export interface ChatModelFacts {
  /** null = unknown (permissive fallback - never gates tools when status is uncertain). */
  supportsToolCalls: boolean | null;
  toolFormat: string | null;
  /** The model's quantisation, from its catalogue entry; the composer says it. */
  quantization: string | null;
}

/**
 * What the chat page knows about the model it is talking to: whether it
 * calls tools, in which format, and its quantisation.
 *
 * Fetched once per model id. A page that moves to another model is remounted
 * with the new id (ModelControlCenterPage keys it by the session), so the
 * answers never describe a model the page is no longer talking to.
 */
export function useChatModelFacts(modelId: number | undefined): ChatModelFacts {
  const [supportsToolCalls, setSupportsToolCalls] = useState<boolean | null>(null);
  const [toolFormat, setToolFormat] = useState<string | null>(null);
  const [quantization, setQuantization] = useState<string | null>(null);
  useEffect(() => {
    // Nothing to ask about remotely: the capability is read from this
    // machine's server registry and the model is on the other machine.
    // `null` is already the permissive answer, which is the right one here.
    if (modelId === undefined) return;
    let cancelled = false;
    getTransport().getServerToolSupport(modelId)
      .then((data) => {
        if (!cancelled) {
          setSupportsToolCalls(data.supports_tool_calls);
          setToolFormat(data.detected_format ?? null);
        }
      })
      .catch(() => {
        // Permissive fallback: leave supportsToolCalls as null (unknown)
      });
    getTransport().getModel(modelId)
      .then((model) => { if (!cancelled) setQuantization(model?.quantization ?? null); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [modelId]);
  return { supportsToolCalls, toolFormat, quantization };
}
