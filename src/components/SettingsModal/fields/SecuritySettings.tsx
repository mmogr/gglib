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

      <SettingField
        id="proxy-host-input"
        label="Endpoint address"
        defaultHint="127.0.0.1"
        description="Literal IP the OpenAI-compatible endpoint binds, for Turn on, the tray, proxy and serve alike; up binds loopback by name. 0.0.0.0 accepts LAN clients and makes the endpoint demand the API key. --host on proxy and serve overrides it for one run."
      >
        <Input
          id="proxy-host-input"
          type="text"
          className="font-mono"
          value={network.proxyHost}
          onChange={(event) => setNetworkSetting('proxyHost', event.target.value)}
          placeholder="127.0.0.1"
          disabled={saving}
          autoComplete="off"
          spellCheck={false}
        />
      </SettingField>

      <SettingField
        id="proxy-allowed-hosts-input"
        label="Allowed hosts"
        defaultHint="none"
        description="Host header values the endpoint accepts beyond loopback and its bound address, comma-separated: a wildcard bind names no reachable address, so a client reaching it by a name or a LAN IP needs that value here."
      >
        <Input
          id="proxy-allowed-hosts-input"
          type="text"
          className="font-mono"
          value={network.proxyAllowedHosts}
          onChange={(event) => setNetworkSetting('proxyAllowedHosts', event.target.value)}
          placeholder="gglib.lan, 192.168.1.20"
          disabled={saving}
          autoComplete="off"
          spellCheck={false}
        />
      </SettingField>

      <ToggleField
        id="proxy-cache-on-disk-input"
        label="Keep the prompt cache on disk"
        checked={network.proxyCacheOnDisk}
        onChange={(value) => setNetworkSetting('proxyCacheOnDisk', value)}
        disabled={saving}
      >
        Keeps prompt caches across model swaps and restarts, in the data folder's slots.
      </ToggleField>

      <SettingField
        id="proxy-cache-disk-gb-input"
        label="Disk budget (GiB)"
        defaultHint="sized from free disk"
        description="How much disk the on-disk prompt cache may hold before its oldest slots are swept, at least 1 GiB. A run started with --cache uses it whether or not the cache is on here."
      >
        <Input
          id="proxy-cache-disk-gb-input"
          type="number"
          min={1}
          className="font-mono"
          value={network.proxyCacheDiskGb}
          onChange={(event) => setNetworkSetting('proxyCacheDiskGb', event.target.value)}
          placeholder="auto"
          disabled={saving}
        />
      </SettingField>
    </>
  );
};
