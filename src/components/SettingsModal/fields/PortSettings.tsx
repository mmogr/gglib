import { FC } from 'react';
import { Input } from '../../ui/Input';
import { SettingField } from './SettingField';

interface PortSettingsProps {
  proxyPortInput: string;
  setProxyPortInput: (value: string) => void;
  serverPortInput: string;
  setServerPortInput: (value: string) => void;
  maxQueueSizeInput: string;
  setMaxQueueSizeInput: (value: string) => void;
  saving: boolean;
}

/**
 * Proxy port, base llama-server port, and download queue size.
 */
export const PortSettings: FC<PortSettingsProps> = ({
  proxyPortInput,
  setProxyPortInput,
  serverPortInput,
  setServerPortInput,
  maxQueueSizeInput,
  setMaxQueueSizeInput,
  saving,
}) => (
  <>
    <SettingField
      id="proxy-port-input"
      label="Proxy Server Port"
      defaultHint="8080"
      description="Port for the OpenAI-compatible proxy server"
    >
      <Input
        id="proxy-port-input"
        type="number"
        value={proxyPortInput}
        onChange={(event) => setProxyPortInput(event.target.value)}
        placeholder="8080"
        min="1024"
        max="65535"
        disabled={saving}
      />
    </SettingField>

    <SettingField
      id="server-port-input"
      label="Base Server Port"
      defaultHint="9000"
      description="Starting port for llama-server instances"
    >
      <Input
        id="server-port-input"
        type="number"
        value={serverPortInput}
        onChange={(event) => setServerPortInput(event.target.value)}
        placeholder="9000"
        min="1024"
        max="65535"
        disabled={saving}
      />
    </SettingField>

    <SettingField
      id="max-queue-size-input"
      label="Max Download Queue Size"
      defaultHint="10"
      description="Maximum number of models that can be queued for download (1-50)"
    >
      <Input
        id="max-queue-size-input"
        type="number"
        value={maxQueueSizeInput}
        onChange={(event) => setMaxQueueSizeInput(event.target.value)}
        placeholder="10"
        min="1"
        max="50"
        disabled={saving}
      />
    </SettingField>
  </>
);
