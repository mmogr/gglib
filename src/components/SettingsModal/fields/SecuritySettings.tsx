import { FC } from 'react';
import { Input } from '../../ui/Input';
import { SecretField, type SecretEdit } from '../../ui/SecretField';
import { useToastContext } from '../../../contexts/ToastContext';
import { PROXY_API_KEY } from '../../../constants/secrets';
import { SettingField } from './SettingField';
import { ToggleField } from './ToggleField';

import type { NetworkSettingsValues } from '../useNetworkSettings';

interface SecuritySettingsProps {
  proxyApiKeyInput: SecretEdit;
  setProxyApiKeyInput: (edit: SecretEdit) => void;
  /** Whether a proxy key is stored; the form never holds the key itself. */
  proxyApiKeySet: boolean;
  network: NetworkSettingsValues;
  setNetworkSetting: <K extends keyof NetworkSettingsValues>(
    key: K,
    value: NetworkSettingsValues[K],
  ) => void;
  saving: boolean;
}

/**
 * Access controls for the proxy endpoint.
 *
 * The key is held, not shown (ADR 0027). The proxy writes a generated one
 * when it binds a non-loopback host, and this dialog is where someone comes
 * to copy it into a client: Copy and Show ask the daemon for it then, so it
 * is on screen only when asked for, and never in what a read of the settings
 * returns.
 */
export const SecuritySettings: FC<SecuritySettingsProps> = ({
  proxyApiKeyInput,
  setProxyApiKeyInput,
  proxyApiKeySet,
  network,
  setNetworkSetting,
  saving,
}) => {
  const { showToast } = useToastContext();
  return (
    <>
      <SettingField
        id="proxy-api-key-input"
        label="Proxy API Key"
        defaultHint="none"
        description="Required on /v1/* and /mcp as 'Authorization: Bearer <key>'. Without one the endpoint is unauthenticated. Set automatically when the proxy binds a non-loopback host."
      >
        <SecretField
          id="proxy-api-key-input"
          label="proxy API key"
          isSet={proxyApiKeySet}
          secret={PROXY_API_KEY}
          edit={proxyApiKeyInput}
          onEdit={setProxyApiKeyInput}
          clearable
          clearNote="A running proxy keeps demanding the old key until it restarts, and a proxy bound off loopback mints a new one."
          placeholder="none — endpoint is open to anything that can reach the port"
          disabled={saving}
          onNotice={showToast}
        />
      </SettingField>

      <SettingField
        id="bind-host-input"
        label="Bind Host"
        defaultHint="127.0.0.1"
        description="Literal IP the daemon binds — a hostname is rejected so the TCP bind and the mDNS record stay unambiguous. The --host flag overrides this for a single run."
      >
        <Input
          id="bind-host-input"
          type="text"
          className="font-mono"
          value={network.bindHost}
          onChange={(event) => setNetworkSetting('bindHost', event.target.value)}
          placeholder="127.0.0.1"
          disabled={saving}
          autoComplete="off"
          spellCheck={false}
        />
      </SettingField>

      <ToggleField
        id="share-lan-input"
        label="Share on the local network"
        checked={network.shareLan}
        onChange={(value) => setNetworkSetting('shareLan', value)}
        disabled={saving}
      >
        Binds the daemon beyond loopback: every device on your network can reach it, and
        its management API can download models and start or stop inference on this
        machine. The API key above is the only thing standing between the network and
        those controls — set one before enabling this.
      </ToggleField>
    </>
  );
};
