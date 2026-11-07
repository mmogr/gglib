import { useState } from 'react';
import { useThread, type ThreadState } from '@assistant-ui/react';
import type { GglibMessageCustom } from '../../../types/messages';
import { useSharedTicker } from './useSharedTicker';

/** The id of the reply waiting for its model to load, or null when none is. */
export function loadingReplyOf(messages: ThreadState['messages']): string | null {
  const last = messages[messages.length - 1];
  if (!last || last.role !== 'assistant' || last.status?.type !== 'running') return null;
  const custom = (last.metadata as { custom?: GglibMessageCustom } | undefined)?.custom;
  return custom?.waiting?.reason === 'model_load' ? last.id : null;
}

const selectLoading = (thread: ThreadState) => loadingReplyOf(thread.messages);

/**
 * Whole seconds the newest reply has waited for its model to load, counted
 * from when this page first saw the wait; null when no reply is waiting.
 * It ticks once a second, and only while there is a wait.
 */
export function useModelLoadWait(): number | null {
  const waiting = useThread({ optional: true, selector: selectLoading }) ?? null;
  const [since, setSince] = useState<{ id: string; at: number } | null>(null);
  if (waiting !== null && since?.id !== waiting) setSince({ id: waiting, at: Date.now() });
  useSharedTicker(waiting !== null, 1000);
  if (waiting === null || since?.id !== waiting) return waiting === null ? null : 0;
  return Math.floor((Date.now() - since.at) / 1000);
}
