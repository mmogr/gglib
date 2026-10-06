/**
 * The signal that the event stream opened.
 *
 * The stream carries no backlog, so whatever was sent while it was down is
 * gone, and the daemon behind a new connection may be a new process. A
 * listener holding state built from events reads that state again when this
 * fires: on the first connection and on every reconnection.
 */

import type { Unsubscribe } from '../types/common';
import { appLogger } from '../../platform';

export class OpenSignal {
  private listeners = new Set<() => void>();

  /** Be told each time the stream opens. Listening does not connect it. */
  listen(fn: () => void): Unsubscribe {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  }

  /** Tell every listener. One that throws does not stop the others. */
  announce(): void {
    for (const fn of this.listeners) {
      try {
        fn();
      } catch (error) {
        appLogger.error('transport.sse', '[SSE] Error in open handler', { error });
      }
    }
  }
}
