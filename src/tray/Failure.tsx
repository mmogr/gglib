/**
 * A failure said inline under the block whose action failed: the popover
 * has no toast host. The daemon's words, cut to fit a 360 px panel with
 * the whole message in the title; a failure with no words says so.
 *
 * @module tray/Failure
 */

import type { FC } from 'react';
import { StatusMark } from '../components/status/StatusMark';

/** Longest failure the panel draws before cutting it. */
const MAX_CHARS = 160;

/** The message as drawn: cut with an ellipsis past `MAX_CHARS`, and never empty. */
export function shownFailure(message: string): string {
  const text = message.trim();
  if (!text) return 'No further detail was reported.';
  return text.length > MAX_CHARS ? `${text.slice(0, MAX_CHARS - 1)}…` : text;
}

export const Failure: FC<{ message: string }> = ({ message }) => (
  <p role="alert" title={message} className="m-0 flex items-baseline gap-xs px-md py-xs text-xs text-text break-words">
    <StatusMark kind="failed" label="Failed" />
    <span className="min-w-0">{shownFailure(message)}</span>
  </p>
);
