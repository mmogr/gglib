/**
 * The page's actions, counted with who ran each, so an error one block shows
 * goes when another block's action runs: a join's refusal is not still on
 * screen after Stop sharing.
 *
 * @module places/devices/pageActions
 */

import { useState } from 'react';

/** The blocks of the page that run actions. */
export type Actor = 'share' | 'paired' | 'join';

/** The page's latest action (how many so far, and who ran it), and the call that counts one more. */
export interface PageActions {
  last: { n: number; by: Actor | null };
  onAct: (by: Actor) => void;
}

/**
 * A block's error line: kept until the block's own next action (which sets
 * or clears it), or until another block's action runs. Who acted decides,
 * not when the count is rendered: a block's own action never clears the
 * error it ends in, however the refusal and the render are ordered.
 */
export function useBlockError({ last }: PageActions, me: Actor) {
  const [error, setError] = useState<string | null>(null);
  const [seen, setSeen] = useState(last.n);
  if (last.n !== seen) {
    setSeen(last.n);
    if (last.by !== me) setError(null);
  }
  return [error, setError] as const;
}
