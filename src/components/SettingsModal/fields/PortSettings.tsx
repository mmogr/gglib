import { FC } from 'react';
import { Input } from '../../ui/Input';
import {
  LLAMA_BASE_PORT,
  MAX_DOWNLOAD_QUEUE_SIZE,
  PROXY_PORT,
} from '../../../constants/settingsDefaults';
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
      controlWidth="xs"
      defaultHint={PROXY_PORT.default}
      description="Port for the OpenAI-compatible proxy server"
    >
      <Input
        id="proxy-port-input"
        type="number"
        value={proxyPortInput}
        onChange={(event) => setProxyPortInput(event.target.value)}
        min={PROXY_PORT.min}
        max={PROXY_PORT.max}
        disabled={saving}
      />
    </SettingField>

    <SettingField
      id="server-port-input"
      label="Base Server Port"
      controlWidth="xs"
      defaultHint={LLAMA_BASE_PORT.default}
      description="Starting port for llama-server instances"
    >
      <Input
        id="server-port-input"
        type="number"
        value={serverPortInput}
        onChange={(event) => setServerPortInput(event.target.value)}
        min={LLAMA_BASE_PORT.min}
        max={LLAMA_BASE_PORT.max}
        disabled={saving}
      />
    </SettingField>

    <SettingField
      id="max-queue-size-input"
      label="Max Download Queue Size"
      controlWidth="xs"
      defaultHint={MAX_DOWNLOAD_QUEUE_SIZE.default}
      description={`Maximum number of models that can be queued for download (${MAX_DOWNLOAD_QUEUE_SIZE.min}-${MAX_DOWNLOAD_QUEUE_SIZE.max})`}
    >
      <Input
        id="max-queue-size-input"
        type="number"
        value={maxQueueSizeInput}
        onChange={(event) => setMaxQueueSizeInput(event.target.value)}
        min={MAX_DOWNLOAD_QUEUE_SIZE.min}
        max={MAX_DOWNLOAD_QUEUE_SIZE.max}
        disabled={saving}
      />
    </SettingField>
  </>
);
