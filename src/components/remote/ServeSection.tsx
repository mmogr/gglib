import { FC, useCallback, useEffect, useRef, useState } from 'react';
import { getTransport } from '../../services/transport';
import { useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { formatError } from '../../utils/errors';
import { Button } from '../ui/Button';
import { Checkbox } from '../ui/Checkbox';
import { Label, Stack } from '../primitives';
import { ProxyStatusPill } from '../proxy';
import { PairingReveal } from './PairingReveal';
import type { RemoteEnableResponse } from '../../services/transport/types/remote';

interface ServeSectionProps {
  onNotice: (message: string, kind: 'success' | 'error' | 'info') => void;
}

/**
 * This machine as the desktop: the tunnel in front of its own proxy.
 *
 * `Enable` brings the tunnel up and shows the ticket and code once, in a
 * `PairingReveal` that leaves when a device pairs or the code expires. The
 * `/mcp` grant is a checkbox that defaults off, because a leaked key with a
 * shell MCP server configured is remote code execution (ADR 0012).
 */
export const ServeSection: FC<ServeSectionProps> = ({ onNotice }) => {
  const { status } = useRemoteState();
  const [allowMcp, setAllowMcp] = useState(false);
  const [busy, setBusy] = useState(false);
  const [reveal, setReveal] = useState<RemoteEnableResponse | null>(null);

  const enabled = status?.enabled ?? false;
  const paired = status?.paired ?? false;

  // The reveal leaves the moment a device pairs, or when the tunnel goes
  // down under it (a `gglib remote disable` in a terminal). Not on a status
  // that merely has not caught up yet: `enable`'s answer arrives before the
  // daemon's event does, and the reveal must be on screen in that gap.
  const wasEnabled = useRef(enabled);
  useEffect(() => {
    if (paired || (wasEnabled.current && !enabled)) setReveal(null);
    wasEnabled.current = enabled;
  }, [paired, enabled]);

  const dropReveal = useCallback(() => setReveal(null), []);

  const handleEnable = async () => {
    setBusy(true);
    try {
      const answer = await getTransport().enableRemote({ allow_mcp: allowMcp });
      setReveal(answer);
      refreshRemoteStatus();
      onNotice(
        'Remote access is on. The local proxy now requires the API key too.',
        'info',
      );
    } catch (err) {
      onNotice(`Could not enable remote access: ${formatError(err)}`, 'error');
    } finally {
      setBusy(false);
    }
  };

  const handleDisable = async () => {
    setBusy(true);
    try {
      await getTransport().disableRemote();
      setReveal(null);
      onNotice('Remote access is off. The ticket is dead.', 'success');
    } catch (err) {
      onNotice(`Could not disable remote access: ${formatError(err)}`, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-labelledby="remote-serve-heading">
      <div className="flex justify-between items-center mb-sm">
        <h4 id="remote-serve-heading" className="m-0 text-sm font-semibold text-text">
          This machine
        </h4>
        <ProxyStatusPill running={enabled} />
      </div>

      {reveal && (
        <PairingReveal
          reveal={reveal}
          onExpired={dropReveal}
          onCopied={() => onNotice('Copied.', 'success')}
        />
      )}

      {enabled ? (
        <Stack gap="sm">
          <dl className="grid grid-cols-[auto_1fr] gap-x-md gap-y-xs text-xs m-0">
            <dt className="text-text-muted">Ticket</dt>
            <dd className="m-0 font-mono">{status?.ticket_fingerprint ?? '—'}</dd>
            <dt className="text-text-muted">Pairing</dt>
            <dd className="m-0">
              {status?.pairing_active
                ? 'code live, waiting for a device'
                : paired
                  ? 'paired'
                  : 'code spent or expired'}
            </dd>
            <dt className="text-text-muted">Peers</dt>
            <dd className="m-0">
              {status?.peers.length
                ? status.peers.map((p) => `${p.fingerprint} (${p.path})`).join(', ')
                : 'none connected'}
            </dd>
            <dt className="text-text-muted">/mcp</dt>
            <dd className="m-0">{status?.mcp_allowed ? 'reachable through the tunnel' : 'not reachable'}</dd>
          </dl>
          <Button variant="danger" className="w-full" onClick={handleDisable} disabled={busy}>
            {busy ? 'Stopping…' : 'Disable remote access'}
          </Button>
        </Stack>
      ) : (
        <Stack gap="sm">
          <Checkbox
            checked={allowMcp}
            onChange={(e) => setAllowMcp(e.target.checked)}
            label="Let the other machine reach /mcp"
            description="Off by default: a leaked key with a shell MCP server configured is remote code execution."
          />
          <Button variant="primary" className="w-full" onClick={handleEnable} disabled={busy}>
            {busy ? 'Finding a relay…' : 'Enable remote access'}
          </Button>
          <Label size="xs" muted>
            Shows a ticket and a six-digit code once. Enabling puts the API key on the local proxy too,
            and disabling does not take that away.
          </Label>
        </Stack>
      )}
    </section>
  );
};
