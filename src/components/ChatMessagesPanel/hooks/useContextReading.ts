import { useMemo } from 'react';
import { useThread, type ThreadState } from '@assistant-ui/react';
import { contextReading, decidingMade, type ContextReading } from '../components/contextReading';

/** The figures the reading comes from: one reply's own, so the same object until they change. */
const decidingFigures = (thread: ThreadState) => decidingMade(thread.messages);

/**
 * The open thread's context reading, for the composer's ring and an image's
 * share of the context; null when the thread gives none, or outside a thread.
 *
 * It reads the thread itself, so nothing is handed down through the page.
 * What it selects keeps its identity while a reply's text arrives, so the
 * reading is worked out again only when the figures change.
 */
export function useContextReading(): ContextReading | null {
  const made = useThread({ optional: true, selector: decidingFigures });
  return useMemo(() => contextReading(made), [made]);
}
