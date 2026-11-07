/**
 * The page's actions, counted, so an error one block shows goes when any
 * other action on the page runs: a join's refusal is not still on screen
 * after Stop sharing.
 *
 * @module places/devices/pageActions
 */

import { useState } from 'react';

/** How many actions the page has started, and the call that counts one more. */
export interface PageActions {
  acted: number;
  onAct: () => void;
}

/**
 * A block's error line: kept until the block's next action, or until another
 * action on the page runs. A block counts its own action before it sets the
 * error that action ends in, so its own error stays.
 */
export function useBlockError({ acted }: PageActions) {
  const [error, setError] = useState<string | null>(null);
  const [seen, setSeen] = useState(acted);
  if (acted !== seen) {
    setSeen(acted);
    setError(null);
  }
  return [error, setError] as const;
}
