/**
 * The Devices place, at `/devices`: this Mac shared with your devices, the
 * devices that hold a key, and another machine used from here (ADR 0012).
 *
 * Every fact is the remote status the registry holds (`remoteRegistry`), read
 * again when the place opens, since nothing emits an event when a device's
 * `last_seen` is written or a terminal forgets one. The words for reach are
 * `REACH_WORDS`, and a device row's description is the daemon's.
 *
 * @module places/devices/DevicesPlace
 */

import { useCallback, useEffect, useState } from 'react';
import { StatusMark } from '../../components/status/StatusMark';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { useRemoteState } from '../../services/remoteRegistry';
import { Note } from '../library/Group';
import { JoinBlock } from './JoinBlock';
import { PairedList } from './PairedList';
import { ShareBlock } from './ShareBlock';
import type { Actor, PageActions } from './pageActions';

export default function DevicesPlace() {
  const { status } = useRemoteState();
  const [last, setLast] = useState<PageActions['last']>({ n: 0, by: null });
  const onAct = useCallback((by: Actor) => setLast((l) => ({ n: l.n + 1, by })), []);
  const actions: PageActions = { last, onAct };

  // Opening asks the daemon again: the status is cheap, and it carries the
  // fields the tunnel's events do not.
  useEffect(() => {
    void refreshRemoteStatus();
  }, []);

  return (
    <section aria-label="Devices" className="flex min-w-0 flex-1 flex-col overflow-hidden">
      {/* As wide as the column below, so the sentence ends where it does. */}
      <header className="flex max-w-[720px] shrink-0 flex-wrap items-baseline gap-sm px-md pb-sm pt-md">
        <h2 className="m-0 flex-1 text-base font-semibold text-text">Devices</h2>
        <span className="text-xs text-text-secondary">End-to-end encrypted. No account.</span>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto pb-md">
        <div className="flex max-w-[720px] flex-col">
          {status === null ? (
            <Note>
              <StatusMark kind="changing" label="Asking gglib" />
            </Note>
          ) : (
            <>
              <ShareBlock status={status} actions={actions} />
              <PairedList status={status} actions={actions} />
              <JoinBlock status={status} actions={actions} />
            </>
          )}
        </div>
      </div>
    </section>
  );
}
