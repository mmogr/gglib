import { FC } from 'react';
import { useMcpServers } from '../../hooks/useMcpServers';
import { Checkbox } from '../ui/Checkbox';
import { Label, Stack } from '../primitives';

interface McpServerPickerProps {
  /** The server names ticked, in the order they were ticked. */
  value: string[];
  onChange: (names: string[]) => void;
}

/**
 * Which MCP servers a paired device's chats may use, by name: a box per
 * server this machine can run, none ticked until ticked. `builtin` is gglib's
 * own tools and an SSE server cannot be run, so neither is offered, and the
 * daemon refuses a name that opens no one server. `/mcp` itself is never
 * reachable through the tunnel, whatever is ticked.
 */
export const McpServerPicker: FC<McpServerPickerProps> = ({ value, onChange }) => {
  const { servers, error } = useMcpServers();
  // One box per name: two servers sharing one are refused by the daemon, and
  // a second box with the same name would be the same choice twice.
  const names = [
    ...new Set(
      servers
        .map((info) => info.server)
        .filter((server) => server.server_type === 'stdio' && server.name !== 'builtin')
        .map((server) => server.name),
    ),
  ];
  const toggle = (name: string, on: boolean) =>
    onChange(on ? [...value, name] : value.filter((n) => n !== name));

  return (
    <Stack gap="xs">
      <Label size="xs" muted>
        MCP servers paired devices&apos; chats may use. A leaked key with a shell server ticked
        is remote code execution.
      </Label>
      {names.map((name) => (
        <Checkbox
          key={name}
          checked={value.includes(name)}
          onChange={(e) => toggle(name, e.target.checked)}
          label={name}
        />
      ))}
      {names.length === 0 && (
        <Label size="xs" muted>
          {error ? `Could not read the MCP servers: ${error}` : 'No MCP server here to offer.'}
        </Label>
      )}
    </Stack>
  );
};
