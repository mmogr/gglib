import { FC, useState } from 'react';
import { getTransport } from '../../services/transport';
import { UNNAMED_PAIRED, pairedName, useRemoteState } from '../../services/remoteRegistry';
import { refreshRemoteStatus } from '../../services/remoteEvents';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { formatError } from '../../utils/errors';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { Label, Stack } from '../primitives';
import { EndpointCopyBar, ProxyStatusPill } from '../proxy';

/** Seconds as a person reads them: `40s`, `3m`, `2h`. */
function awayFor(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  return `${Math.floor(secs / 3600)}h`;
}

interface JoinSectionProps {
  onNotice: (message: string, kind: 'success' | 'error' | 'info') => void;
}

/**
 * This machine as the laptop: a loopback port here that is another machine.
 *
 * First time, the whole `<ticket>-<code>` string; afterwards the ticket, or
 * nothing to dial the last one. Once connected the port is shown the way the
 * proxy's is, with the reminder that a client pointed there supplies the key
 * itself — the port does not inject it (ADR 0012, decision 7).
 *
 * That machine's models are not chosen here: they are in the model library,
 * under its name, beside this machine's, and a chat with one starts from its
 * row there.
 */
export const JoinSection: FC<JoinSectionProps> = ({ onNotice }) => {
  const { status } = useRemoteState();
  const { confirm } = useConfirmContext();
  const [pairing, setPairing] = useState('');
  const [busy, setBusy] = useState(false);

  const connected = status?.connected ?? null;
  // The fingerprint says whether a pairing is stored; the name is what is
  // shown of it. A fingerprint is never rendered.
  const stored = (status?.stored_ticket_fingerprint ?? null) !== null;
  const machine = pairedName(status);
  const hasKey = status?.has_remote_key ?? false;
  const canReuse = stored && hasKey;

  const handleJoin = async () => {
    setBusy(true);
    try {
      const trimmed = pairing.trim();
      const answer = await getTransport().joinRemote(trimmed ? { pairing: trimmed } : {});
      setPairing('');
      refreshRemoteStatus();
      const joined = answer.name ?? UNNAMED_PAIRED;
      onNotice(
        answer.paired ? `Joined ${joined}, and paired with it. Its key is stored here.` : `Joined ${joined}.`,
        'success',
      );
    } catch (err) {
      onNotice(`Could not join: ${formatError(err)}`, 'error');
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
    <section aria-labelledby="remote-join-heading">
      <div className="flex justify-between items-center mb-sm">
        <h4 id="remote-join-heading" className="m-0 text-sm font-semibold text-text">
          Another machine
        </h4>
        <ProxyStatusPill running={connected !== null} />
      </div>

      {connected ? (
        <Stack gap="sm">
          {/*
            The connection names nobody until the status read lands — the
            join event carries a port and nothing else, so neither the peer
            nor its name is known yet. Naming nobody is the honest reading of
            that; "Connected to  (idle)." reads as a bug.
          */}
          <p className="text-xs text-text-muted m-0">
            {connected.ticket_fingerprint ? (
              <>
                Connected to <span className="font-medium text-text-secondary">{machine}</span>{' '}
                {connected.away_for_s === null
                  ? `(${connected.path}).`
                  : `— away ${awayFor(connected.away_for_s)}; the address stays, and it reconnects when that machine is back.`}
              </>
            ) : (
              'Connected. Reading which machine…'
            )}
          </p>
          <Stack gap="xs">
            <Label size="xs" muted>That machine, from here</Label>
            <EndpointCopyBar host="127.0.0.1" port={connected.port} onCopied={() => onNotice('Copied.', 'success')} />
            <Label size="xs" muted>A client pointed there supplies its API key; gglib’s own chat does.</Label>
            <Label size="xs" muted>
              That key is the one this machine was given when it paired;{' '}
              <code className="font-mono">gglib remote key --show</code> prints it.
            </Label>
          </Stack>
          <Label size="xs" muted>
            Its models are in the library, under its name; chat with one from its row there.
          </Label>
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
                canReuse ? `Leave empty to dial ${machine} again` : '<ticket>-<code> from the other machine'
              }
              onChange={(e) => setPairing(e.target.value)}
            />
          </div>
          <Button
            variant="primary"
            className="w-full"
            onClick={handleJoin}
            disabled={busy || (!pairing.trim() && !canReuse)}
          >
            {busy ? 'Reaching it…' : 'Join'}
          </Button>
          {stored && !hasKey && (
            <Label size="xs" muted>
              Last dialled {machine}, but no key is stored — pair again with the full string.
            </Label>
          )}
        </Stack>
      )}
    </section>
  );
};
