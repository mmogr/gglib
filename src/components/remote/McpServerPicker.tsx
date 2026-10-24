import { FC, useCallback } from 'react';
import { useMcpServers } from '../../hooks/useMcpServers';
import type { McpServerId } from '../../services/transport';
import { canOpenToDevices } from '../../utils/mcp';
import { Label, Stack } from '../primitives';
import { AllowRemoteCheckbox } from './AllowRemoteCheckbox';

interface McpServerPickerProps {
  /** Called once a box has changed a server, so what is shown beside it is read again. */
  onChanged: () => void;
}

/**
 * Which MCP servers paired devices' chats may use: a box per server this
 * machine can open, each the server's own flag, set at once whether remote
 * access is on or off and read by a device's next run. `builtin` is gglib's
 * own tools and an SSE server cannot be run, so neither is offered. `/mcp`
 * itself is never reachable through the tunnel, whatever is ticked.
 */
export const McpServerPicker: FC<McpServerPickerProps> = ({ onChanged }) => {
  const { servers, error, setAllowRemote } = useMcpServers();
  const openable = servers.map((info) => info.server).filter(canOpenToDevices);
  const toggle = useCallback(
    async (id: McpServerId, allow: boolean) => {
      await setAllowRemote(id, allow);
      onChanged();
    },
    [setAllowRemote, onChanged],
  );

  return (
    <Stack gap="xs">
      <Label size="xs" muted>
        MCP servers paired devices&apos; chats may use. A leaked key with a shell server ticked
        is remote code execution.
      </Label>
      {openable.map((server) => (
        <AllowRemoteCheckbox
          key={server.id}
          server={server}
          label={server.name}
          onToggle={toggle}
        />
      ))}
      {openable.length === 0 && (
        <Label size="xs" muted>
          {error ? `Could not read the MCP servers: ${error}` : 'No MCP server here to offer.'}
        </Label>
      )}
    </Stack>
  );
};
