import { FC } from 'react';
import { Input } from '../../ui/Input';
import { SettingField } from './SettingField';

interface SecuritySettingsProps {
  proxyApiKeyInput: string;
  setProxyApiKeyInput: (value: string) => void;
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
  saving,
}) => (
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
);
