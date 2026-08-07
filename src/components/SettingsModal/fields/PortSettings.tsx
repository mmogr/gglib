import { FC } from 'react';
import {
  LLAMA_BASE_PORT,
  MAX_DOWNLOAD_QUEUE_SIZE,
  PROXY_PORT,
} from '../../../constants/settingsDefaults';
import { NumberSettingField } from './NumberSettingField';

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
    <NumberSettingField
      id="proxy-port-input"
      label="Proxy Server Port"
      spec={PROXY_PORT}
      value={proxyPortInput}
      onChange={setProxyPortInput}
      description="Port for the OpenAI-compatible proxy server"
      disabled={saving}
    />

    <NumberSettingField
      id="server-port-input"
      label="Base Server Port"
      spec={LLAMA_BASE_PORT}
      value={serverPortInput}
      onChange={setServerPortInput}
      description="Starting port for llama-server instances"
      disabled={saving}
    />

    <NumberSettingField
      id="max-queue-size-input"
      label="Max Download Queue Size"
      spec={MAX_DOWNLOAD_QUEUE_SIZE}
      value={maxQueueSizeInput}
      onChange={setMaxQueueSizeInput}
      description={`Maximum number of models that can be queued for download (${MAX_DOWNLOAD_QUEUE_SIZE.min}-${MAX_DOWNLOAD_QUEUE_SIZE.max})`}
      disabled={saving}
    />
  </>
);
