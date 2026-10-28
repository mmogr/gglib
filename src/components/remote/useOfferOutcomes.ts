import { useEffect, useRef } from 'react';
import { useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { useToastContext } from '../../contexts/ToastContext';

/**
 * Say how a pairing code ended, wherever the person is.
 *
 * Mounted once at the app's root rather than in the popover: a code is often
 * spent or lapses while the popover is closed, or while a chat has taken the
 * page, and the person is looking at their phone either way. The CLI's
 * pairing screen says both when it leaves.
 *
 * Only a pairing and an expiry are said. A withdrawal or a tunnel taken down
 * comes from the app's own Forget or Disable, which say so themselves; from a
 * terminal, where the person who typed it already knows; or from the proxy
 * stopping or the daemon shutting down, which take the tunnel with them.
 *
 * An expiry also asks the daemon again. It lets go of a code with no event,
 * so without the read the status keeps saying one is live, and Invite, which
 * is disabled while one is, stays dead.
 */
export function useOfferOutcomes(): void {
  const { ended } = useRemoteState();
  const { showToast } = useToastContext();
  // Starts at whatever ended before this mounted, which is not news.
  const said = useRef(ended?.seq ?? 0);

  useEffect(() => {
    if (ended === null || ended.seq <= said.current) return;
    said.current = ended.seq;
    if (ended.reason === 'paired') {
      showToast(
        ended.device ? `Paired ${ended.device}. It is listed under Devices.` : 'A device paired.',
        'success',
      );
    } else if (ended.reason === 'expired') {
      showToast('The pairing code expired and nobody paired. Invite again for a fresh one.', 'info');
      void refreshRemoteStatus();
    }
  }, [ended, showToast]);
}
