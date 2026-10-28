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
 * pairing screen says the same when it leaves.
 *
 * Every ending is said but a withdrawal, which is a Forget: the app's own
 * says so itself, and a terminal's was typed by the person who knows. A
 * tunnel taken down by the app's own Disable is not an ending at all
 * (`clearOffer`), so what is said of one is the proxy stopping, the daemon
 * shutting down, or a terminal's disable.
 *
 * An expiry also asks the daemon again. It lets go of a code with no event,
 * so without the read the status keeps saying one is live, and Invite, which
 * is disabled while one is, stays dead. A lapse needs no read: a read is how
 * it was found.
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
    } else if (ended.reason === 'lapsed') {
      showToast(
        'The pairing code stopped working before anyone paired. Invite again for a fresh one.',
        'info',
      );
    } else if (ended.reason === 'tunnel_down') {
      showToast('Remote access went off, and the pairing code with it. Nobody paired.', 'info');
    }
  }, [ended, showToast]);
}
