import { FC } from 'react';
import { Input } from '../../ui/Input';
import { SettingField } from './SettingField';
import { ToggleField } from './ToggleField';

import type { NetworkSettingsValues } from '../useNetworkSettings';

interface SecuritySettingsProps {
  proxyApiKeyInput: string;
  setProxyApiKeyInput: (value: string) => void;
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
 * Shown as a plain text input rather than a password field: the proxy writes
 * a generated key here when it binds a non-loopback host, and this dialog is
 * then the place someone comes to read it and paste it into a client. Masking
 * it would defeat the only reason to open this field.
 */
export const SecuritySettings: FC<SecuritySettingsProps> = ({
  proxyApiKeyInput,
  setProxyApiKeyInput,
  network,
  setNetworkSetting,
  saving,
}) => (
  <>
  <SettingField
    id="proxy-api-key-input"
    label="Proxy API Key"
    defaultHint="none"
    description="Required on /v1/* and /mcp as 'Authorization: Bearer <key>'. Leave empty for an unauthenticated endpoint. Set automatically when the proxy binds a non-loopback host."
  >
    <Input
      id="proxy-api-key-input"
      type="text"
      value={proxyApiKeyInput}
      onChange={(event) => setProxyApiKeyInput(event.target.value)}
      placeholder="none — endpoint is open to anything that can reach the port"
      disabled={saving}
      autoComplete="off"
      spellCheck={false}
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
