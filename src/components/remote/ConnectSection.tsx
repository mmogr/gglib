import { FC, useState } from 'react';
import { getTransport } from '../../services/transport';
import { setUseRemoteForChat, useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { formatError } from '../../utils/errors';
import { Button } from '../ui/Button';
import { Checkbox } from '../ui/Checkbox';
import { Input } from '../ui/Input';
import { Label, Stack } from '../primitives';
import { EndpointCopyBar, ProxyStatusPill } from '../proxy';

interface ConnectSectionProps {
  onNotice: (message: string, kind: 'success' | 'error' | 'info') => void;
}

/**
 * This machine as the laptop: a loopback port here that is another machine.
 *
 * First time, the whole `<ticket>-<code>` string; afterwards the ticket, or
 * nothing to dial the last one. Once connected the port is shown the way the
 * proxy's is, with the reminder that a client pointed there supplies the key
 * itself — the port does not inject it (ADR 0012, decision 7). `Use for chat`
 * is this window's choice and is cleared when the connection goes.
 */
export const ConnectSection: FC<ConnectSectionProps> = ({ onNotice }) => {
  const { status, useForChat } = useRemoteState();
  const { confirm } = useConfirmContext();
  const [pairing, setPairing] = useState('');
  const [busy, setBusy] = useState(false);

  const connected = status?.connected ?? null;
  const storedFingerprint = status?.stored_ticket_fingerprint ?? null;
  const hasKey = status?.has_remote_key ?? false;
  const canReuse = storedFingerprint !== null && hasKey;

  const handleConnect = async () => {
    setBusy(true);
    try {
      const trimmed = pairing.trim();
      const answer = await getTransport().connectRemote(trimmed ? { pairing: trimmed } : {});
      setPairing('');
      refreshRemoteStatus();
      onNotice(
        answer.paired
          ? `Paired with ${answer.ticket_fingerprint}. Its key is stored here.`
          : `Connected to ${answer.ticket_fingerprint}.`,
        'success',
      );
    } catch (err) {
      onNotice(`Could not connect: ${formatError(err)}`, 'error');
    } finally {
      setBusy(false);
    }
  };

  const handleDisconnect = async () => {
    setBusy(true);
    try {
      await getTransport().disconnectRemote();
      onNotice('Disconnected. The pairing is remembered.', 'success');
    } catch (err) {
      onNotice(`Could not disconnect: ${formatError(err)}`, 'error');
    } finally {
      setBusy(false);
    }
  };

  const handleKill = async () => {
    const ok = await confirm({
      title: 'Stop the remote daemon?',
      description:
        'This stops gglib on the other machine — its proxy, its models, its downloads. ' +
        'Nothing can start it again from here.',
      confirmLabel: 'Stop it',
      variant: 'danger',
    });
    if (!ok) return;
    setBusy(true);
    try {
      await getTransport().killRemote();
      onNotice('The remote daemon is stopping; this side is disconnected.', 'info');
    } catch (err) {
      onNotice(`Could not stop the remote: ${formatError(err)}`, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <section aria-labelledby="remote-connect-heading">
      <div className="flex justify-between items-center mb-sm">
        <h4 id="remote-connect-heading" className="m-0 text-sm font-semibold text-text">
          Another machine
        </h4>
        <ProxyStatusPill running={connected !== null} />
      </div>

      {connected ? (
        <Stack gap="sm">
          <p className="text-xs text-text-muted m-0">
            Connected to <span className="font-mono text-text-secondary">{connected.ticket_fingerprint}</span>{' '}
            ({connected.path}).
          </p>
          <Stack gap="xs">
            <Label size="xs" muted>That machine, from here</Label>
            <EndpointCopyBar host="127.0.0.1" port={connected.port} onCopied={() => onNotice('Copied.', 'success')} />
            <Label size="xs" muted>A client pointed there supplies its API key; gglib’s own chat does.</Label>
          </Stack>
          <Checkbox
            checked={useForChat}
            onChange={(e) => setUseRemoteForChat(e.target.checked)}
            label="Use it for chat"
            description="Chat turns go to the other machine instead of a model here."
          />
          <Button variant="secondary" className="w-full" onClick={handleDisconnect} disabled={busy}>
            Disconnect
          </Button>
          <Button variant="danger" className="w-full" onClick={handleKill} disabled={busy}>
            Stop the remote daemon
          </Button>
        </Stack>
      ) : (
        <Stack gap="sm">
          <div>
            <Label size="xs" muted className="mb-xs" htmlFor="remote-pairing">
              Pairing string
            </Label>
            <Input
              id="remote-pairing"
              type="text"
              className="font-mono"
              value={pairing}
              placeholder={
                canReuse ? `Leave empty to dial ${storedFingerprint} again` : '<ticket>-<code> from the other machine'
              }
              onChange={(e) => setPairing(e.target.value)}
            />
          </div>
          <Button
            variant="primary"
            className="w-full"
            onClick={handleConnect}
            disabled={busy || (!pairing.trim() && !canReuse)}
          >
            {busy ? 'Reaching it…' : 'Connect'}
          </Button>
          {storedFingerprint && !hasKey && (
            <Label size="xs" muted>
              Last dialled {storedFingerprint}, but no key is stored — pair again with the full string.
            </Label>
          )}
        </Stack>
      )}
    </section>
  );
};
