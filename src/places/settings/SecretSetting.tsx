/**
 * The endpoint's API key, saved as you go and held, not shown (ADR 0027).
 *
 * A read never carries the key, only whether one is stored, so a stored key
 * shows `MASK`; Show and Copy ask the daemon's one reveal each time. Replace
 * opens a box and asks before it writes; Clear writes at once. Either way
 * the key it replaces is revealed first, so Undo can write it back: a write
 * whose old key cannot be read is not made. With nothing stored the box
 * alone sets one. The write sends `proxyApiKey` and no other field.
 *
 * @module places/settings/SecretSetting
 */

import { useCallback, useState, type FC } from 'react';
import { Copy, Eye, EyeOff } from 'lucide-react';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { Copied, useCopied } from '../../components/CopyValue';
import { useToastContext } from '../../contexts/ToastContext';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { IconButton } from '../../components/ui/IconButton';
import { Input } from '../../components/ui/Input';
import { MASK, copySecret, revealed } from '../../components/ui/SecretField';
import { SecretInput } from '../../components/ui/SecretInput';
import { PROXY_API_KEY } from '../../constants/secrets';
import { useSettings } from '../../hooks/useSettings';
import { formatError } from '../../utils/errors';
import { SettingField } from './SettingField';
import { useUndoableWrite } from './useFieldSave';

const ID = 'proxy-api-key-input';
const LABEL = 'API key';

export const SecretSetting: FC<{ note: string }> = ({ note }) => {
  const { settings, save } = useSettings();
  const { confirm } = useConfirmContext();
  const { showToast } = useToastContext();
  const isSet = settings?.hasProxyApiKey === true;
  const write = useCallback(
    (value: string | null) => save({ proxyApiKey: value }, { quiet: true }),
    [save],
  );
  const field = useUndoableWrite<string | null>(null, write);
  const [typing, setTyping] = useState<string | null>(null);
  const [shown, setShown] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [copied, showCopied] = useCopied();

  /** Write `next` over the stored key, which is read first for Undo. */
  const replaceWith = async (next: string | null) => {
    setShown(null);
    setRefusal(null);
    let before: string | null = null;
    if (isSet) {
      try {
        before = await revealed(PROXY_API_KEY);
      } catch (err) {
        setRefusal(`The key in use could not be read, so it was left as it is: ${formatError(err)}`);
        return;
      }
    }
    if (await field.commit(next, before)) setTyping(null);
  };

  const submit = async () => {
    const next = typing?.trim();
    if (!next) return;
    if (isSet) {
      const yes = await confirm({
        title: 'Replace the API key?',
        description: 'Clients that send the old key are refused within a few seconds.',
        confirmLabel: 'Replace',
      });
      if (!yes) return;
    }
    await replaceWith(next);
  };

  const toggleShown = () => {
    if (shown !== null) {
      setShown(null);
      return;
    }
    revealed(PROXY_API_KEY)
      .then(setShown)
      .catch((err) => showToast(`Could not show the ${LABEL}: ${formatError(err)}`, 'error'));
  };
  const copy = () => {
    copySecret(PROXY_API_KEY)
      .then(showCopied)
      .catch((err) => showToast(`Could not copy the ${LABEL}: ${formatError(err)}`, 'error'));
  };

  const status = { error: refusal ?? field.error, undo: field.undo };
  const busy = field.saving;

  return (
    <SettingField
      id={ID}
      label={LABEL}
      defaultHint="none"
      description="Required on /v1/* and /mcp as 'Authorization: Bearer <key>'. Without one the endpoint is open to anything that can reach the port. Set automatically when the endpoint binds a non-loopback address."
      note={note}
      save={status}
    >
      {!isSet || typing !== null ? (
        <div className="flex items-center gap-sm">
          <SecretInput
            id={ID}
            label={LABEL}
            value={typing ?? ''}
            onChange={setTyping}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void submit();
              if (event.key === 'Escape' && isSet) setTyping(null);
            }}
            placeholder={isSet ? 'The new key' : 'none'}
            disabled={busy}
          />
          <Button variant="secondary" size="sm" disabled={busy || !typing?.trim()} onClick={() => void submit()}>
            {isSet ? 'Replace' : 'Set'}
          </Button>
          {isSet && (
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => setTyping(null)}>
              Cancel
            </Button>
          )}
        </div>
      ) : (
        <div className="flex items-center gap-sm">
          <Input id={ID} className="min-w-0 flex-1 font-mono" value={shown ?? MASK} aria-label={LABEL} readOnly />
          <IconButton label={shown === null ? `Show the ${LABEL}` : `Hide the ${LABEL}`} size="sm" onClick={toggleShown}>
            <Icon icon={shown === null ? Eye : EyeOff} size={14} />
          </IconButton>
          <Copied shown={copied} />
          <IconButton label={`Copy the ${LABEL}`} size="sm" onClick={copy}>
            <Icon icon={Copy} size={14} />
          </IconButton>
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => setTyping('')}>
            Replace
          </Button>
          <Button variant="dangerGhost" size="sm" disabled={busy} onClick={() => void replaceWith(null)}>
            Clear
          </Button>
        </div>
      )}
    </SettingField>
  );
};
