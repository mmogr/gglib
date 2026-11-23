/**
 * What the page says while it waits out Hugging Face's rate limit, the time
 * left as a countdown (`utils/countdown`'s `clock`).
 *
 * @module components/status/hubWait
 */

import { clock } from '../../utils/countdown';

/** In place of a read or a search the Hub refused: the page's own words, with no Rust twin. */
export function liftsLine(seconds: number): string {
  return `Hugging Face's rate limit lifts in ${clock(seconds)}. gglib will read this again then.`;
}
