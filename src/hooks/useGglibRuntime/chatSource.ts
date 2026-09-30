/**
 * Which machine's chats the runtime reads: this one's, or the far machine's
 * through this machine's daemon. One source at a time; the runs of each are
 * asked of that source alone.
 *
 * @module chatSource
 */

import { getTransport, type ChatSource } from '../../services/transport';
import type { RunStreamItem } from '../../services/transport/api/runs';
import type { GglibContent } from '../../types/messages';
import type { RunInfo } from '../../types/generated/RunInfo';

/** A source's runs: list, cancel, and read one's events. */
export interface SourceRuns {
  listRuns(): Promise<RunInfo[]>;
  cancelRun(id: string): Promise<RunInfo>;
  readRunEvents(id: string, after: number, signal: AbortSignal): AsyncGenerator<RunStreamItem>;
}

/** The runs of `source`. This machine's are asked as they always were. */
export function runsOf(source: ChatSource): SourceRuns {
  const transport = getTransport();
  if (source === 'far') {
    return {
      listRuns: () => transport.listFarRuns(),
      cancelRun: (id) => transport.cancelFarRun(id),
      readRunEvents: (id, after, signal) => transport.readFarRunEvents(id, after, signal),
    };
  }
  return {
    listRuns: () => transport.listRuns(),
    cancelRun: (id) => transport.cancelRun(id),
    readRunEvents: (id, after, signal) => transport.readRunEvents(id, after, signal),
  };
}

/** The text a far turn carries: its text parts, joined. */
export function turnText(content: GglibContent): string {
  if (typeof content === 'string') return content;
  return content
    .map((part) => (part.type === 'text' ? part.text : ''))
    .filter(Boolean)
    .join('\n\n');
}
